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
    "usage:\n  rss-backend --version\n  rss-backend doctor\n  rss-backend http-probe --url HTTPS_URL\n  rss-backend init-probe-db --db PATH\n  rss-backend materialize-fixture --out PATH\n  rss-backend --db PATH feed add URL\n  rss-backend --db PATH feed list\n  rss-backend --db PATH feed remove ID\n  rss-backend --db PATH init-fixture-db\n  rss-backend --db PATH device-probe --cache DIR\n  rss-backend --db PATH refresh [--feed ID] [--budget SEC]\n  rss-backend --db PATH materialize ID --cache DIR\n  rss-backend --db PATH mark ID read|unread"
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

fn run(arguments: &[String]) -> Result<(), Error> {
    if arguments.first().map(String::as_str) == Some("--db") {
        let db = path_argument(&arguments[..2], "--db")?;
        let command = arguments.get(2).map(String::as_str);
        let mut store = store::Store::open(&db)?;
        match command {
            Some("feed")
                if arguments.get(3).map(String::as_str) == Some("add") && arguments.len() == 5 =>
            {
                let id = store.add_feed(&arguments[4], unix_now())?;
                println!("{id}");
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
                if arguments.get(3).map(String::as_str) == Some("list") && arguments.len() == 4 =>
            {
                for feed in store.list_feeds()? {
                    println!(
                        "{}\t{}\t{}\t{}",
                        feed.id,
                        if feed.enabled { "enabled" } else { "disabled" },
                        feed.title.as_deref().unwrap_or(""),
                        feed.source_url
                    );
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
                let cache_files = std::fs::read_dir(&cache)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .filter_map(|entry| entry.metadata().ok().map(|meta| (entry, meta)))
                    .filter(|(entry, _)| entry.path().extension().and_then(|ext| ext.to_str()) == Some("html"))
                    .filter(|(_, meta)| meta.is_file())
                    .collect::<Vec<_>>();
                let cache_bytes: u64 = cache_files.iter().map(|(_, meta)| meta.len()).sum();
                println!("schema_version={}", store.schema_version()?);
                println!("journal_mode={}", store.journal_mode()?);
                println!("database_bytes={}", store.database_bytes()?);
                println!("freelist_pages={}", store.freelist_pages()?);
                println!("article_count={}", store.article_count()?);
                println!("cache_files={}", cache_files.len());
                println!("cache_bytes={cache_bytes}");
            }
            Some("refresh") => {
                let feed_id = integer_argument(&arguments[3..], "--feed", 0)?;
                let budget = integer_argument(&arguments[3..], "--budget", 240)?;
                if feed_id == 0 {
                    let _ = refresh::run_all(&mut store, budget)?;
                } else {
                    let _ = refresh::run(&mut store, feed_id as i64, budget)?;
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
            Some("mark") if arguments.len() == 5 => {
                let id = arguments[3]
                    .parse::<i64>()
                    .map_err(|_| Error::message("invalid article ID"))?;
                let read = match arguments[4].as_str() {
                    "read" => true,
                    "unread" => false,
                    _ => return Err(Error::message(usage())),
                };
                if !store.mark_read(id, read, unix_now())? {
                    return Err(Error::message("article not found"));
                }
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
        ])
        .expect("add feed");
        run(&["--db".into(), db_arg.clone(), "feed".into(), "list".into()]).expect("list feeds");
        run(&["--db".into(), db_arg.clone(), "init-fixture-db".into()]).expect("fixture database");
        run(&[
            "--db".into(),
            db_arg.clone(),
            "device-probe".into(),
            "--cache".into(),
            directory.path().display().to_string(),
        ])
        .expect("device probe");
    }
}
