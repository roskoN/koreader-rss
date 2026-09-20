//! Bounded synchronous feed refresh vertical slice.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::article;
use crate::feed::{parse_feed, ParsedEntry};
use crate::http::{FeedRequest, FeedResponse, HttpClient};
use crate::store::{ArticleInsert, Store};
use crate::Error;

// Each worker may hold a page, extracted HTML, image data, and compressed
// blob simultaneously. Keep this conservative for the Kindle's limited RAM.
const ARTICLE_DOWNLOAD_THREADS: usize = 2;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn retry_after(error: &Error) -> Option<i64> {
    match error {
        Error::Http { retry_after_s, .. } => *retry_after_s,
        _ => None,
    }
}

pub struct RefreshLock {
    path: PathBuf,
}

impl RefreshLock {
    pub fn acquire(database: &Path) -> Result<Self, Error> {
        let path = database.with_extension("refresh.lock");
        let stamp = now();
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                use std::io::Write;
                let mut file = file;
                writeln!(file, "{} {}", std::process::id(), stamp)?;
                Ok(Self { path })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let stale = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| {
                        text.split_whitespace()
                            .nth(1)
                            .and_then(|value| value.parse::<i64>().ok())
                    })
                    .is_some_and(|started| stamp.saturating_sub(started) > 900);
                if stale {
                    std::fs::remove_file(&path)?;
                    return Self::acquire(database);
                }
                Err(Error::message("another refresh is already running"))
            }
            Err(error) => Err(error.into()),
        }
    }
}

impl Drop for RefreshLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn run_with_reason(
    store: &mut Store,
    feed_id: i64,
    budget_s: u64,
    reason: i64,
) -> Result<(usize, usize), Error> {
    let started_at = now();
    let run_id = store.begin_refresh_run(reason, budget_s, started_at)?;
    let result = run_one(store, feed_id, budget_s);
    let (inserted, failures) = match &result {
        Ok(counts) => *counts,
        Err(_) => (0, 1),
    };
    if let Some(feed) = store.feed(Some(feed_id))? {
        store.record_feed_attempt(run_id, &feed, inserted, failures)?;
    }
    store.finish_refresh_run(
        run_id,
        now(),
        if result.is_ok() && failures == 0 {
            crate::store::RUN_OUTCOME_SUCCESS
        } else {
            crate::store::RUN_OUTCOME_FAILED
        },
        result.as_ref().err().map(ToString::to_string).as_deref(),
    )?;
    result
}

fn run_one(store: &mut Store, feed_id: i64, budget_s: u64) -> Result<(usize, usize), Error> {
    let feed = store
        .feed(Some(feed_id))?
        .ok_or_else(|| Error::message("feed not found or disabled"))?;
    let started = std::time::Instant::now();
    let fetched_at = now();
    let response = HttpClient::new().fetch_feed(FeedRequest {
        url: &feed.source_url,
        etag: feed.http_etag.as_deref(),
        last_modified: feed.http_last_modified.as_deref(),
    });
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            store.update_feed_failure_with_retry(
                feed.id,
                &error.to_string(),
                fetched_at,
                retry_after(&error),
            )?;
            return Err(error);
        }
    };
    if budget_s > 0 && started.elapsed() > Duration::from_secs(budget_s) {
        return Err(Error::message("refresh budget exhausted"));
    }
    match response {
        FeedResponse::NotModified {
            effective_url,
            etag,
            last_modified,
        } => {
            store.update_feed_success(
                feed.id,
                feed.title.as_deref(),
                &effective_url,
                etag.as_deref(),
                last_modified.as_deref(),
                fetched_at,
                fetched_at + 21_600,
            )?;
            Ok((0, 0))
        }
        FeedResponse::Body {
            effective_url,
            etag,
            last_modified,
            bytes,
        } => {
            let parsed = match parse_feed(&bytes) {
                Ok(parsed) => parsed,
                Err(error) => {
                    store.update_feed_failure(feed.id, &error.to_string(), fetched_at)?;
                    return Err(error);
                }
            };
            // The parser owns all fields needed below; release the full feed
            // response before downloading article pages.
            drop(bytes);
            let mut inserted = 0;
            let mut failures = 0;
            download_entries(&feed, parsed.entries, |result| {
                match result.prepared {
                    Ok(prepared) => {
                        if let Some(error) = result.fallback_error {
                            store.record_entry_failure(
                                feed.id,
                                &result.entry,
                                fetched_at,
                                &error,
                            )?;
                        }
                        if insert_prepared_entry(store, &feed, &result.entry, fetched_at, prepared)?
                        {
                            inserted += 1;
                        }
                    }
                    Err(error) => {
                        failures += 1;
                        store.record_entry_failure(feed.id, &result.entry, fetched_at, &error)?;
                    }
                }
                Ok(())
            })?;
            if failures == 0 {
                store.update_feed_success(
                    feed.id,
                    parsed.title.as_deref(),
                    &effective_url,
                    etag.as_deref(),
                    last_modified.as_deref(),
                    fetched_at,
                    fetched_at + 21_600,
                )?;
                let _ = store.prune(fetched_at)?;
            } else {
                store.update_feed_failure(
                    feed.id,
                    "one or more entries could not be stored",
                    fetched_at,
                )?;
            }
            Ok((inserted, failures))
        }
    }
}

pub fn run_all_with_reason(
    store: &mut Store,
    budget_s: u64,
    reason: i64,
) -> Result<(usize, usize), Error> {
    let run_id = store.begin_refresh_run(reason, budget_s, now())?;
    let started = std::time::Instant::now();
    let mut inserted = 0;
    let mut failures = 0;
    for feed_id in store.due_feeds(now())? {
        if budget_s > 0 && started.elapsed() >= Duration::from_secs(budget_s) {
            break;
        }
        let remaining_budget = if budget_s == 0 {
            0
        } else {
            budget_s.saturating_sub(started.elapsed().as_secs())
        };
        let feed = store.feed(Some(feed_id))?;
        match run_one(store, feed_id, remaining_budget) {
            Ok((new, failed)) => {
                inserted += new;
                failures += failed;
                if let Some(feed) = feed.as_ref() {
                    store.record_feed_attempt(run_id, feed, new, failed)?;
                }
            }
            Err(error) => {
                failures += 1;
                if let Some(feed) = feed.as_ref() {
                    store.record_feed_attempt(run_id, feed, 0, 1)?;
                }
                let _ = error;
            }
        }
    }
    store.finish_refresh_run(
        run_id,
        now(),
        if failures == 0 {
            crate::store::RUN_OUTCOME_SUCCESS
        } else {
            crate::store::RUN_OUTCOME_PARTIAL
        },
        None,
    )?;
    Ok((inserted, failures))
}

struct PreparedEntry {
    source_kind: i64,
    uncompressed_size: usize,
    blob: Vec<u8>,
}

struct DownloadResult {
    entry: ParsedEntry,
    prepared: Result<PreparedEntry, String>,
    fallback_error: Option<String>,
}

fn download_entries(
    feed: &crate::store::FeedRow,
    entries: Vec<ParsedEntry>,
    mut on_result: impl FnMut(DownloadResult) -> Result<(), Error>,
) -> Result<(), Error> {
    if entries.is_empty() {
        return Ok(());
    }
    let queue = Arc::new(Mutex::new(entries.into_iter().collect::<VecDeque<_>>()));
    let worker_count = ARTICLE_DOWNLOAD_THREADS.min(queue.lock().expect("queue lock").len());
    // Bound completed results so workers cannot accumulate every article's
    // prepared blob faster than SQLite can persist it.
    let (sender, receiver) = mpsc::sync_channel(worker_count);
    let mut workers = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let queue = Arc::clone(&queue);
        let sender = sender.clone();
        let feed = feed.clone();
        workers.push(thread::spawn(move || {
            let client = HttpClient::new();
            loop {
                let Some(entry) = queue.lock().expect("queue lock").pop_front() else {
                    break;
                };
                let result = prepare_entry(&feed, entry, &client);
                if sender.send(result).is_err() {
                    break;
                }
            }
        }));
    }
    drop(sender);
    let mut callback_error = None;
    for result in receiver {
        if callback_error.is_none() {
            if let Err(error) = on_result(result) {
                callback_error = Some(error);
            }
        }
    }
    for worker in workers {
        worker.join().expect("article download worker");
    }
    callback_error.map_or(Ok(()), Err)
}

fn prepare_entry(
    feed: &crate::store::FeedRow,
    mut source: ParsedEntry,
    client: &HttpClient,
) -> DownloadResult {
    let mut source_kind = 1;
    let mut fallback_error = None;
    if let Some(url) = source.url.as_deref() {
        // Prefer the canonical article page over RSS summaries/content. Keep
        // usable RSS content if the page is unavailable, so one paywalled or
        // broken article does not discard an otherwise valid feed entry.
        match client.fetch_page(url).and_then(|(effective, page)| {
            let body = String::from_utf8(page).map_err(|_| Error::message("page is not UTF-8"))?;
            let content = article::extract_page_with_selectors(
                &body,
                &effective,
                feed.content_selector.as_deref(),
                feed.remove_selector.as_deref(),
            )?;
            Ok((content, effective))
        }) {
            Ok((content, _effective)) => {
                source.content = Some(content);
                source_kind = 2;
            }
            Err(error)
                if source
                    .content
                    .as_deref()
                    .is_some_and(|content| !content.trim().is_empty()) =>
            {
                fallback_error = Some(format!(
                    "full article unavailable; used RSS content: {error}"
                ));
            }
            Err(error) => {
                return DownloadResult {
                    entry: source,
                    prepared: Err(error.to_string()),
                    fallback_error: None,
                };
            }
        }
    } else if source.content.as_deref().is_none() {
        return DownloadResult {
            entry: source,
            prepared: Err("RSS entry has neither content nor article URL".to_owned()),
            fallback_error: None,
        };
    }
    let html = match article::wrap(&source) {
        Ok(html) => html,
        Err(error) => {
            return DownloadResult {
                entry: source,
                prepared: Err(error.to_string()),
                fallback_error,
            };
        }
    };
    let html = article::embed_images(&html, source.url.as_deref(), client);
    let blob = match article::compress(&html) {
        Ok(blob) => blob,
        Err(error) => {
            return DownloadResult {
                entry: source,
                prepared: Err(error.to_string()),
                fallback_error,
            };
        }
    };
    let uncompressed_size = html.len();
    source.content = None;
    source.summary = None;
    DownloadResult {
        entry: source,
        prepared: Ok(PreparedEntry {
            source_kind,
            uncompressed_size,
            blob,
        }),
        fallback_error,
    }
}

fn insert_prepared_entry(
    store: &mut Store,
    feed: &crate::store::FeedRow,
    entry: &ParsedEntry,
    fetched_at: i64,
    prepared: PreparedEntry,
) -> Result<bool, Error> {
    let PreparedEntry {
        source_kind,
        uncompressed_size,
        blob,
    } = prepared;
    let changed = store.insert_article(&ArticleInsert {
        feed_id: feed.id,
        dedupe_key: &entry.dedupe_key,
        guid: (!entry.id.is_empty()).then_some(entry.id.as_str()),
        url: entry.url.as_deref(),
        title: entry.title.as_deref().unwrap_or("(untitled)"),
        author: entry.authors.first().map(String::as_str),
        published_at: entry.published_at,
        sort_at: entry
            .published_at
            .or(entry.updated_at)
            .unwrap_or(fetched_at),
        fetched_at,
        source_kind,
        compression_codec: article::COMPRESSION_CODEC,
        content_format: article::CONTENT_FORMAT,
        storage_version: article::STORAGE_VERSION,
        uncompressed_size,
        content_blob: &blob,
    })?;
    if changed {
        store.clear_entry_failure(feed.id, &entry.dedupe_key)?;
    }
    Ok(changed)
}

#[cfg(test)]
fn extract_body(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    if let (Some(start), Some(end)) = (lower.find("<article"), lower.rfind("</article>")) {
        return html[start..end + 10].to_owned();
    }
    if let (Some(start), Some(end)) = (lower.find("<body"), lower.rfind("</body>")) {
        return html[start..end + 7].to_owned();
    }
    html.to_owned()
}

#[cfg(test)]
mod tests {
    use super::{extract_body, RefreshLock};

    #[test]
    fn extracts_article_or_body_without_network() {
        assert_eq!(
            extract_body("<html><article>story</article></html>"),
            "<article>story</article>"
        );
        assert_eq!(
            extract_body("<html><body>story</body></html>"),
            "<body>story</body>"
        );
    }

    #[test]
    fn refresh_lock_rejects_overlap_and_cleans_up() {
        let directory = tempfile::tempdir().expect("directory");
        let database = directory.path().join("rss.sqlite3");
        let lock = RefreshLock::acquire(&database).expect("lock");
        assert!(RefreshLock::acquire(&database).is_err());
        drop(lock);
        assert!(RefreshLock::acquire(&database).is_ok());
    }
}
