mod article;
mod doctor;
mod feed;
mod fixture;
mod http;
mod http_probe;
mod probe;
mod refresh;
mod store;
mod version;

use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Image(image::ImageError),
    Http {
        message: String,
        retry_after_s: Option<i64>,
    },
    Message(String),
}

impl Error {
    fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Sqlite(error) => error.fmt(formatter),
            Self::Image(error) => error.fmt(formatter),
            Self::Http { message, .. } => formatter.write_str(message),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<image::ImageError> for Error {
    fn from(error: image::ImageError) -> Self {
        Self::Image(error)
    }
}

fn usage() -> &'static str {
    "usage:\n  rss-backend --version\n  rss-backend doctor\n  rss-backend http-probe --url HTTPS_URL\n  rss-backend init-probe-db --db PATH\n  rss-backend materialize-fixture --out PATH\n  rss-backend --db PATH feed add URL\n  rss-backend --db PATH feed list\n  rss-backend --db PATH feed enable ID\n  rss-backend --db PATH feed disable ID\n  rss-backend --db PATH feed remove ID\n  rss-backend --db PATH init-fixture-db\n  rss-backend --db PATH device-probe --cache DIR\n  rss-backend --db PATH status\n  rss-backend --db PATH refresh [--feed ID] [--budget SEC] [--unbounded] [--reason manual|wake]\n  rss-backend --db PATH materialize ID --cache DIR"
}

fn value_argument(arguments: &[String], flag: &str) -> Result<String, Error> {
    if arguments.len() == 2 && arguments[0] == flag {
        Ok(arguments[1].clone())
    } else {
        Err(Error::message(usage()))
    }
}

fn path_argument(arguments: &[String], flag: &str) -> Result<PathBuf, Error> {
    Ok(PathBuf::from(value_argument(arguments, flag)?))
}

fn integer_argument(arguments: &[String], flag: &str, default: u64) -> Result<u64, Error> {
    match arguments.iter().position(|argument| argument == flag) {
        None => Ok(default),
        Some(index) => arguments
            .get(index + 1)
            .ok_or_else(|| Error::message(usage()))?
            .parse()
            .map_err(|_| Error::message(format!("invalid value for {flag}"))),
    }
}

fn machine_field(value: &str) -> String {
    value.replace(['\t', '\r', '\n'], " ")
}

fn opml_urls(xml: &str) -> Vec<String> {
    let lower = xml.to_ascii_lowercase();
    let mut urls = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = lower[cursor..].find("<outline") {
        let start = cursor + relative;
        let Some(end_relative) = lower[start..].find('>') else {
            break;
        };
        let tag = &xml[start..start + end_relative + 1];
        let tag_lower = tag.to_ascii_lowercase();
        if let Some(attr) = tag_lower.find("xmlurl=") {
            let value_start = attr + 7;
            let quote = tag_lower
                .as_bytes()
                .get(value_start)
                .copied()
                .unwrap_or(b' ');
            if quote == b'"' || quote == b'\'' {
                let content_start = value_start + 1;
                if let Some(end) = tag_lower[content_start..].find(quote as char) {
                    let url = tag[content_start..content_start + end].trim();
                    if !url.is_empty() && !urls.iter().any(|known| known == url) {
                        urls.push(url.to_owned());
                    }
                }
            }
        }
        cursor = start + end_relative + 1;
    }
    urls
}

fn import_opml_dir(store: &mut store::Store, directory: &std::path::Path) -> Result<usize, Error> {
    let mut imported = 0;
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("opml"))
            != Some(true)
        {
            continue;
        }
        let xml = std::fs::read_to_string(&path)?;
        for url in opml_urls(&xml) {
            if url.starts_with("https://") || url.starts_with("http://") {
                let title = http::HttpClient::new()
                    .fetch_feed(http::FeedRequest {
                        url: &url,
                        etag: None,
                        last_modified: None,
                    })
                    .ok()
                    .and_then(|response| match response {
                        http::FeedResponse::Body { bytes, .. } => feed::parse_feed(&bytes)
                            .ok()
                            .and_then(|parsed| parsed.title),
                        http::FeedResponse::NotModified { .. } => None,
                    });
                store.add_feed_with_title(&url, title.as_deref(), unix_now())?;
                imported += 1;
            }
        }
        std::fs::remove_file(path)?;
    }
    Ok(imported)
}

fn run(arguments: &[String]) -> Result<(), Error> {
    if arguments.first().map(String::as_str) == Some("--db") {
        let db = path_argument(&arguments[..2], "--db")?;
        let command = arguments.get(2).map(String::as_str);
        let mut store = store::Store::open(&db)?;
        match command {
            Some("feed")
                if arguments.get(3).map(String::as_str) == Some("add")
                    && (arguments.len() == 5 || arguments.len() == 6) =>
            {
                let title = arguments
                    .get(5)
                    .filter(|title| !title.is_empty())
                    .map(String::as_str);
                let id = store.add_feed_with_title(&arguments[4], title, unix_now())?;
                println!("{id}");
            }
            Some("feed")
                if arguments.get(3).map(String::as_str) == Some("check")
                    && arguments.len() == 5 =>
            {
                let url = &arguments[4];
                if !(url.starts_with("https://") || url.starts_with("http://")) {
                    return Err(Error::message("feed URL must use http:// or https://"));
                }
                let response = http::HttpClient::new().fetch_feed(http::FeedRequest {
                    url,
                    etag: None,
                    last_modified: None,
                })?;
                let bytes = match response {
                    http::FeedResponse::Body { bytes, .. } => bytes,
                    http::FeedResponse::NotModified { .. } => {
                        return Err(Error::message(
                            "feed returned not-modified without validators",
                        ))
                    }
                };
                let parsed = feed::parse_feed(&bytes)?;
                println!(
                    "ok\tentries={}\ttitle={}",
                    parsed.entries.len(),
                    machine_field(parsed.title.as_deref().unwrap_or(""))
                );
            }
            Some("feed")
                if arguments.get(3).map(String::as_str) == Some("import-opml")
                    && arguments.len() == 5 =>
            {
                println!(
                    "imported={}",
                    import_opml_dir(&mut store, std::path::Path::new(&arguments[4]))?
                );
            }
            Some("feed")
                if arguments.get(3).map(String::as_str) == Some("remove")
                    && arguments.len() == 5 =>
            {
                let id = arguments[4]
                    .parse::<i64>()
                    .map_err(|_| Error::message("invalid feed ID"))?;
                if !store.remove_feed(id)? {
                    return Err(Error::message("feed not found"));
                }
            }
            Some("feed")
                if arguments.get(3).map(String::as_str) == Some("remove-all")
                    && arguments.len() == 4 =>
            {
                println!("removed={}", store.remove_all_feeds()?);
            }
            Some("articles")
                if arguments.get(3).map(String::as_str) == Some("remove-all")
                    && arguments.len() == 4 =>
            {
                println!("removed={}", store.remove_all_articles()?);
            }
            Some("feed")
                if arguments.get(3).map(String::as_str) == Some("list") && arguments.len() == 4 =>
            {
                for feed in store.list_feeds()? {
                    println!(
                        "{}\t{}\t{}\t{}\t{}\t{}",
                        feed.id,
                        if feed.enabled { "enabled" } else { "disabled" },
                        machine_field(feed.title.as_deref().unwrap_or("")),
                        machine_field(&feed.source_url),
                        feed.failure_count,
                        machine_field(feed.last_error.as_deref().unwrap_or(""))
                    );
                }
            }
            Some("feed")
                if matches!(
                    arguments.get(3).map(String::as_str),
                    Some("enable" | "disable")
                ) && arguments.len() == 5 =>
            {
                let id = arguments[4]
                    .parse::<i64>()
                    .map_err(|_| Error::message("invalid feed ID"))?;
                let enabled = arguments[3] == "enable";
                if !store.set_feed_enabled(id, enabled)? {
                    return Err(Error::message("feed not found"));
                }
            }
            Some("feed")
                if arguments.get(3).map(String::as_str) == Some("selectors")
                    && arguments.len() == 7 =>
            {
                let id = arguments[4]
                    .parse::<i64>()
                    .map_err(|_| Error::message("invalid feed ID"))?;
                if !store.set_feed_selectors(
                    id,
                    (!arguments[5].is_empty()).then_some(arguments[5].as_str()),
                    (!arguments[6].is_empty()).then_some(arguments[6].as_str()),
                )? {
                    return Err(Error::message("feed not found"));
                }
            }
            Some("init-fixture-db") if arguments.len() == 3 => {
                let feed_id = store.add_feed("https://example.org/fixture-feed", unix_now())?;
                let html = fixture::html(fixture::DEFAULT_TITLE)?;
                let blob = article::compress(&html)?;
                let fetched_at = unix_now();
                store.insert_article(&store::ArticleInsert {
                    feed_id,
                    dedupe_key: "g:milestone0-fixture",
                    guid: Some("milestone0-fixture"),
                    url: Some("https://example.org/fixture"),
                    title: fixture::DEFAULT_TITLE,
                    author: Some("rss-backend"),
                    published_at: Some(fetched_at),
                    sort_at: fetched_at,
                    fetched_at,
                    source_kind: 1,
                    compression_codec: article::COMPRESSION_CODEC,
                    content_format: article::CONTENT_FORMAT,
                    storage_version: article::STORAGE_VERSION,
                    uncompressed_size: html.len(),
                    content_blob: &blob,
                })?;
                println!(
                    "fixture feed_id={feed_id} article_count={}",
                    store.article_count()?
                );
            }
            Some("device-probe") if arguments.len() == 5 && arguments[3] == "--cache" => {
                let cache = PathBuf::from(&arguments[4]);
                let (cache_files, cache_bytes) = std::fs::read_dir(&cache)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry.path().extension().and_then(|ext| ext.to_str()) == Some("html")
                    })
                    .filter_map(|entry| entry.metadata().ok())
                    .filter(|meta| meta.is_file())
                    .fold((0usize, 0u64), |(count, bytes), meta| {
                        (count + 1, bytes + meta.len())
                    });
                println!("schema_version={}", store.schema_version()?);
                println!("journal_mode={}", store.journal_mode()?);
                println!("database_bytes={}", store.database_bytes()?);
                println!("freelist_pages={}", store.freelist_pages()?);
                println!("article_count={}", store.article_count()?);
                println!("cache_files={cache_files}");
                println!("cache_bytes={cache_bytes}");
            }
            Some("status") if arguments.len() == 3 => {
                if let Some(run) = store.latest_refresh_run()? {
                    println!("run_id={}", run.id);
                    println!("started_at={}", run.started_at);
                    println!("finished_at={}", run.finished_at);
                    println!("feeds_checked={}", run.feeds_checked);
                    println!("new_articles={}", run.new_articles);
                    println!("failed_feeds={}", run.failed_feeds);
                    println!("budget_s={}", run.budget_s);
                    println!("reason={}", run.reason);
                    println!(
                        "outcome={}",
                        run.outcome
                            .map_or_else(|| "running".to_owned(), |value| value.to_string())
                    );
                    println!("last_error={}", run.last_error.unwrap_or_default());
                } else {
                    println!("run_id=none");
                }
            }
            Some("refresh") => {
                let feed_id = integer_argument(&arguments[3..], "--feed", 0)?;
                let reason = match arguments.iter().position(|argument| argument == "--reason") {
                    None => store::RUN_REASON_MANUAL,
                    Some(index) => match arguments.get(index + 1).map(String::as_str) {
                        Some("wake") => 2,
                        Some("manual") => store::RUN_REASON_MANUAL,
                        _ => return Err(Error::message("invalid refresh reason")),
                    },
                };
                let unbounded = arguments.iter().any(|argument| argument == "--unbounded");
                let budget = if unbounded {
                    0
                } else {
                    integer_argument(
                        &arguments[3..],
                        "--budget",
                        if reason == store::RUN_REASON_MANUAL {
                            0
                        } else {
                            240
                        },
                    )?
                };
                let _lock = refresh::RefreshLock::acquire(&db)?;
                if reason == 2 {
                    if let Some(last) = store.last_successful_refresh_at()? {
                        let age = unix_now().saturating_sub(last);
                        let interval = store.refresh_interval_s()?.max(0);
                        if age < interval {
                            let run = store.begin_refresh_run(reason, budget, unix_now())?;
                            store.finish_refresh_run(
                                run,
                                unix_now(),
                                store::RUN_OUTCOME_SKIPPED,
                                Some("minimum refresh interval has not elapsed"),
                            )?;
                            println!("skipped=recent_success age_s={age} interval_s={interval}");
                            return Ok(());
                        }
                    }
                }
                if feed_id == 0 {
                    let _ = refresh::run_all_with_reason(&mut store, budget, reason)?;
                } else {
                    let _ = refresh::run_with_reason(&mut store, feed_id as i64, budget, reason)?;
                }
            }
            Some("materialize")
                if arguments.len() == 6
                    && arguments[3].parse::<i64>().is_ok()
                    && arguments[4] == "--cache" =>
            {
                let id = arguments[3]
                    .parse::<i64>()
                    .map_err(|_| Error::message(usage()))?;
                let cache = PathBuf::from(&arguments[5]);
                let content = store
                    .article_content(id)?
                    .ok_or_else(|| Error::message("article not found"))?;
                if content.compression_codec != article::COMPRESSION_CODEC {
                    return Err(Error::message("unsupported article compression codec"));
                }
                let path = article::materialize(
                    id,
                    content.storage_version,
                    content.fetched_at,
                    &content.content_blob,
                    content.uncompressed_size,
                    &cache,
                )?;
                println!("{}", path.display());
            }
            _ => return Err(Error::message(usage())),
        }
        return Ok(());
    }
    match arguments.first().map(String::as_str) {
        Some("--version") if arguments.len() == 1 => println!("{}", version::banner()),
        Some("doctor") if arguments.len() == 1 => {
            if !doctor::run() {
                return Err(Error::message("doctor critical checks failed"));
            }
        }
        Some("http-probe") => {
            let url = value_argument(&arguments[1..], "--url")?;
            let status = http_probe::run(&url)?;
            println!("https ok status={status} url={url}");
        }
        Some("init-probe-db") => {
            let path = probe::initialize(&path_argument(&arguments[1..], "--db")?)?;
            if !probe::probe_present(&path)? || probe::journal_mode(&path)? != "delete" {
                return Err(Error::message("probe database validation failed"));
            }
            println!("{}", path.display());
        }
        Some("materialize-fixture") => {
            let path = fixture::materialize(&path_argument(&arguments[1..], "--out")?)?;
            println!("{}", path.display());
        }
        _ => return Err(Error::message(usage())),
    }
    Ok(())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rss-backend: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_command() {
        assert!(run(&["refresh".to_owned()]).is_err());
    }

    #[test]
    fn requires_exact_path_flag() {
        assert!(path_argument(&[], "--db").is_err());
        assert!(path_argument(&["--wrong".to_owned(), "x".to_owned()], "--db").is_err());
    }

    #[test]
    fn feed_cli_and_fixture_database_commands_are_repeatable() {
        let directory = tempfile::tempdir().expect("directory");
        let db = directory.path().join("rss.sqlite3");
        let db_arg = db.display().to_string();
        run(&[
            "--db".into(),
            db_arg.clone(),
            "feed".into(),
            "add".into(),
            "https://example.org/feed".into(),
            "Example title".into(),
        ])
        .expect("add feed");
        let verify_store = store::Store::open(&db).expect("open verification store");
        assert_eq!(
            verify_store
                .feed(Some(1))
                .expect("feed lookup")
                .expect("feed")
                .title,
            Some("Example title".to_owned())
        );
        run(&["--db".into(), db_arg.clone(), "feed".into(), "list".into()]).expect("list feeds");
        run(&[
            "--db".into(),
            db_arg.clone(),
            "feed".into(),
            "disable".into(),
            "1".into(),
        ])
        .expect("disable feed");
        run(&[
            "--db".into(),
            db_arg.clone(),
            "feed".into(),
            "enable".into(),
            "1".into(),
        ])
        .expect("enable feed");
        run(&["--db".into(), db_arg.clone(), "init-fixture-db".into()]).expect("fixture database");
        run(&["--db".into(), db_arg.clone(), "status".into()]).expect("status");
        let opml_dir = directory.path().join("feeds");
        std::fs::create_dir(&opml_dir).expect("opml directory");
        std::fs::write(
            opml_dir.join("subscriptions.opml"),
            r#"<opml><body><outline text="Example" xmlUrl="https://example.org/feed.xml" /></body></opml>"#,
        )
        .expect("opml");
        run(&[
            "--db".into(),
            db_arg.clone(),
            "feed".into(),
            "import-opml".into(),
            opml_dir.display().to_string(),
        ])
        .expect("import opml");
        assert!(!opml_dir.join("subscriptions.opml").exists());
        run(&[
            "--db".into(),
            db_arg.clone(),
            "device-probe".into(),
            "--cache".into(),
            directory.path().display().to_string(),
        ])
        .expect("device probe");
        run(&[
            "--db".into(),
            db_arg.clone(),
            "articles".into(),
            "remove-all".into(),
        ])
        .expect("remove articles");
        run(&["--db".into(), db_arg, "feed".into(), "remove-all".into()]).expect("remove feeds");
    }
}
