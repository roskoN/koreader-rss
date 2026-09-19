//! Bounded synchronous feed refresh vertical slice.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::article;
use crate::feed::{parse_feed, ParsedEntry};
use crate::http::{FeedRequest, FeedResponse, HttpClient};
use crate::store::{ArticleInsert, Store};
use crate::Error;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn run(store: &mut Store, feed_id: i64, budget_s: u64) -> Result<(usize, usize), Error> {
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
            store.update_feed_failure(feed.id, &error.to_string(), fetched_at)?;
            return Err(error);
        }
    };
    if started.elapsed() > Duration::from_secs(budget_s) {
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
            let mut inserted = 0;
            let mut failures = 0;
            for entry in &parsed.entries {
                match insert_entry(store, feed.id, entry, fetched_at) {
                    Ok(true) => inserted += 1,
                    Ok(false) => {}
                    Err(error) => {
                        failures += 1;
                        store.record_entry_failure(
                            feed.id,
                            entry,
                            fetched_at,
                            &error.to_string(),
                        )?;
                    }
                }
            }
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

fn insert_entry(
    store: &mut Store,
    feed_id: i64,
    entry: &ParsedEntry,
    fetched_at: i64,
) -> Result<bool, Error> {
    let html = article::wrap(entry)?;
    let blob = article::compress(&html)?;
    let changed = store.insert_article(&ArticleInsert {
        feed_id,
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
        source_kind: 1,
        compression_codec: article::COMPRESSION_CODEC,
        content_format: article::CONTENT_FORMAT,
        storage_version: article::STORAGE_VERSION,
        uncompressed_size: html.len(),
        content_blob: &blob,
    })?;
    if changed {
        store.clear_entry_failure(feed_id, &entry.dedupe_key)?;
    }
    Ok(changed)
}
