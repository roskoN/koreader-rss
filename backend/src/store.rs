//! SQLite authority for feeds, article metadata, content, and update state.

use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::feed::ParsedEntry;
use crate::Error;

pub const APPLICATION_ID: i64 = 0x5253_5352; // "RSSR"
pub const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FeedRow {
    pub id: i64,
    pub source_url: String,
    pub effective_url: Option<String>,
    pub title: Option<String>,
    pub http_etag: Option<String>,
    pub http_last_modified: Option<String>,
    pub next_due_at: i64,
}

#[derive(Debug)]
pub struct ArticleInsert<'a> {
    pub feed_id: i64,
    pub dedupe_key: &'a str,
    pub guid: Option<&'a str>,
    pub url: Option<&'a str>,
    pub title: &'a str,
    pub author: Option<&'a str>,
    pub published_at: Option<i64>,
    pub sort_at: i64,
    pub fetched_at: i64,
    pub source_kind: i64,
    pub compression_codec: i64,
    pub content_format: i64,
    pub storage_version: i64,
    pub uncompressed_size: usize,
    pub content_blob: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ArticleSummary {
    pub id: i64,
    pub title: String,
    pub feed_title: String,
    pub sort_at: i64,
    pub is_read: bool,
}

pub struct Store {
    connection: Connection,
}

#[allow(dead_code)]
impl Store {
    pub fn open(path: &Path) -> Result<Self, Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(3))?;
        connection.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA synchronous=FULL;
             PRAGMA foreign_keys=ON;
             PRAGMA cache_size=-2048;
             PRAGMA mmap_size=0;
             PRAGMA secure_delete=OFF;",
        )?;
        let mut store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<(), Error> {
        let version: i64 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(Error::message(format!(
                "database schema {version} is newer than supported schema {SCHEMA_VERSION}"
            )));
        }
        if version == 0 {
            self.connection.execute_batch(
                "PRAGMA page_size=4096;
                 PRAGMA auto_vacuum=INCREMENTAL;",
            )?;
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(
                "CREATE TABLE app_state (
                    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                    scheduler_cursor INTEGER NOT NULL DEFAULT 0,
                    last_maintenance_at INTEGER
                 );
                 INSERT INTO app_state(singleton) VALUES (1);

                 CREATE TABLE settings (
                    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                    default_refresh_s INTEGER NOT NULL DEFAULT 21600,
                    retention_days INTEGER NOT NULL DEFAULT 90,
                    max_articles_per_feed INTEGER NOT NULL DEFAULT 500,
                    max_articles_total INTEGER NOT NULL DEFAULT 5000,
                    max_db_bytes INTEGER NOT NULL DEFAULT 268435456,
                    cache_max_files INTEGER NOT NULL DEFAULT 3,
                    cache_max_bytes INTEGER NOT NULL DEFAULT 33554432
                 );
                 INSERT INTO settings(singleton) VALUES (1);

                 CREATE TABLE feeds (
                    id INTEGER PRIMARY KEY,
                    source_url TEXT NOT NULL UNIQUE,
                    effective_url TEXT,
                    site_url TEXT,
                    title TEXT,
                    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),
                    content_policy INTEGER NOT NULL DEFAULT 0,
                    content_selector TEXT,
                    remove_selector TEXT,
                    include_images INTEGER NOT NULL DEFAULT 1 CHECK (include_images IN (0,1)),
                    refresh_interval_s INTEGER NOT NULL DEFAULT 21600,
                    max_articles INTEGER,
                    schedule_order INTEGER NOT NULL UNIQUE,
                    http_etag TEXT,
                    http_last_modified TEXT,
                    http_expires_at INTEGER,
                    last_attempt_at INTEGER,
                    last_checked_at INTEGER,
                    last_success_at INTEGER,
                    next_due_at INTEGER NOT NULL DEFAULT 0,
                    backoff_until INTEGER,
                    failure_count INTEGER NOT NULL DEFAULT 0,
                    last_error TEXT,
                    created_at INTEGER NOT NULL
                 );

                 CREATE TABLE articles (
                    id INTEGER PRIMARY KEY,
                    feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
                    dedupe_key TEXT NOT NULL,
                    guid TEXT,
                    url TEXT,
                    title TEXT NOT NULL,
                    author TEXT,
                    published_at INTEGER,
                    sort_at INTEGER NOT NULL,
                    fetched_at INTEGER NOT NULL,
                    source_kind INTEGER NOT NULL,
                    is_read INTEGER NOT NULL DEFAULT 0 CHECK (is_read IN (0,1)),
                    read_at INTEGER,
                    compression_codec INTEGER NOT NULL,
                    content_format INTEGER NOT NULL,
                    storage_version INTEGER NOT NULL,
                    uncompressed_size INTEGER NOT NULL,
                    compressed_size INTEGER NOT NULL,
                    content_blob BLOB NOT NULL,
                    UNIQUE(feed_id, dedupe_key),
                    CHECK (compressed_size = length(content_blob))
                 );

                 CREATE TABLE entry_failures (
                    feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
                    dedupe_key TEXT NOT NULL,
                    url TEXT,
                    title TEXT,
                    attempt_count INTEGER NOT NULL,
                    last_attempt_at INTEGER NOT NULL,
                    next_retry_at INTEGER NOT NULL,
                    source_token TEXT,
                    is_permanent INTEGER NOT NULL DEFAULT 0 CHECK (is_permanent IN (0,1)),
                    last_error TEXT NOT NULL,
                    PRIMARY KEY(feed_id, dedupe_key)
                 ) WITHOUT ROWID;

                 CREATE TABLE refresh_runs (
                    id INTEGER PRIMARY KEY,
                    reason INTEGER NOT NULL,
                    started_at INTEGER NOT NULL,
                    finished_at INTEGER,
                    budget_s INTEGER,
                    feeds_checked INTEGER NOT NULL DEFAULT 0,
                    new_articles INTEGER NOT NULL DEFAULT 0,
                    failed_feeds INTEGER NOT NULL DEFAULT 0,
                    outcome INTEGER,
                    last_error TEXT
                 );

                 CREATE INDEX idx_feeds_due ON feeds(enabled, next_due_at, schedule_order);
                 CREATE INDEX idx_articles_newest ON articles(sort_at DESC, id DESC);
                 CREATE INDEX idx_articles_unread ON articles(is_read, sort_at DESC, id DESC);
                 CREATE INDEX idx_articles_feed ON articles(feed_id, sort_at DESC, id DESC);
                 CREATE INDEX idx_entry_failures_retry ON entry_failures(next_retry_at);
                 CREATE INDEX idx_refresh_runs_started ON refresh_runs(started_at DESC);",
            )?;
            transaction.pragma_update(None, "application_id", APPLICATION_ID)?;
            transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            transaction.commit()?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64, Error> {
        Ok(self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    pub fn journal_mode(&self) -> Result<String, Error> {
        Ok(self
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?)
    }

    pub fn add_feed(&mut self, url: &str, now: i64) -> Result<i64, Error> {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(Error::message("feed URL must use http:// or https://"));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let schedule_order: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(schedule_order), -1) + 1 FROM feeds",
            [],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO feeds(source_url,schedule_order,created_at) VALUES (?1,?2,?3)",
            params![url, schedule_order, now],
        )?;
        let id = transaction.query_row(
            "SELECT id FROM feeds WHERE source_url=?1",
            params![url],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn feed(&self, id: Option<i64>) -> Result<Option<FeedRow>, Error> {
        let sql = if id.is_some() {
            "SELECT id,source_url,effective_url,title,http_etag,http_last_modified,next_due_at
             FROM feeds WHERE id=?1 AND enabled=1"
        } else {
            "SELECT id,source_url,effective_url,title,http_etag,http_last_modified,next_due_at
             FROM feeds WHERE enabled=1 ORDER BY schedule_order LIMIT 1"
        };
        let mut statement = self.connection.prepare(sql)?;
        let row = if let Some(id) = id {
            statement
                .query_row(params![id], Self::map_feed)
                .optional()?
        } else {
            statement.query_row([], Self::map_feed).optional()?
        };
        Ok(row)
    }

    fn map_feed(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeedRow> {
        Ok(FeedRow {
            id: row.get(0)?,
            source_url: row.get(1)?,
            effective_url: row.get(2)?,
            title: row.get(3)?,
            http_etag: row.get(4)?,
            http_last_modified: row.get(5)?,
            next_due_at: row.get(6)?,
        })
    }

    pub fn insert_article(&mut self, article: &ArticleInsert<'_>) -> Result<bool, Error> {
        if article.dedupe_key.is_empty()
            || article.title.is_empty()
            || article.uncompressed_size == 0
            || article.content_blob.is_empty()
        {
            return Err(Error::message(
                "article metadata and content must be complete",
            ));
        }
        let uncompressed_size = i64::try_from(article.uncompressed_size)
            .map_err(|_| Error::message("uncompressed article is too large"))?;
        let compressed_size = i64::try_from(article.content_blob.len())
            .map_err(|_| Error::message("compressed article is too large"))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "INSERT OR IGNORE INTO articles(
                feed_id,dedupe_key,guid,url,title,author,published_at,sort_at,fetched_at,
                source_kind,compression_codec,content_format,storage_version,
                uncompressed_size,compressed_size,content_blob
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![
                article.feed_id,
                article.dedupe_key,
                article.guid,
                article.url,
                article.title,
                article.author,
                article.published_at,
                article.sort_at,
                article.fetched_at,
                article.source_kind,
                article.compression_codec,
                article.content_format,
                article.storage_version,
                uncompressed_size,
                compressed_size,
                article.content_blob,
            ],
        )?;
        transaction.commit()?;
        Ok(changed == 1)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_feed_success(
        &mut self,
        feed_id: i64,
        title: Option<&str>,
        effective_url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
        now: i64,
        next_due_at: i64,
    ) -> Result<(), Error> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE feeds SET
                title=COALESCE(?2,title), effective_url=?3, http_etag=?4,
                http_last_modified=?5, last_attempt_at=?6, last_checked_at=?6,
                last_success_at=?6, next_due_at=?7, backoff_until=NULL,
                failure_count=0, last_error=NULL
             WHERE id=?1",
            params![
                feed_id,
                title,
                effective_url,
                etag,
                last_modified,
                now,
                next_due_at
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn article_count(&self) -> Result<i64, Error> {
        Ok(self
            .connection
            .query_row("SELECT count(*) FROM articles", [], |row| row.get(0))?)
    }

    pub fn update_feed_failure(
        &mut self,
        feed_id: i64,
        error: &str,
        now: i64,
    ) -> Result<(), Error> {
        self.connection.execute(
            "UPDATE feeds SET last_attempt_at=?2,last_checked_at=?2,failure_count=failure_count+1,last_error=?3,backoff_until=?2+900,next_due_at=?2+900 WHERE id=?1",
            params![feed_id, now, error],
        )?;
        Ok(())
    }

    pub fn record_entry_failure(
        &mut self,
        feed_id: i64,
        entry: &ParsedEntry,
        now: i64,
        error: &str,
    ) -> Result<(), Error> {
        self.connection.execute(
            "INSERT INTO entry_failures(feed_id,dedupe_key,url,title,attempt_count,last_attempt_at,next_retry_at,last_error)
             VALUES(?1,?2,?3,?4,1,?5,?5+900,?6)
             ON CONFLICT(feed_id,dedupe_key) DO UPDATE SET attempt_count=attempt_count+1,last_attempt_at=excluded.last_attempt_at,next_retry_at=excluded.next_retry_at,last_error=excluded.last_error",
            params![feed_id, entry.dedupe_key, entry.url, entry.title, now, error],
        )?;
        Ok(())
    }

    pub fn clear_entry_failure(&mut self, feed_id: i64, dedupe_key: &str) -> Result<(), Error> {
        self.connection.execute(
            "DELETE FROM entry_failures WHERE feed_id=?1 AND dedupe_key=?2",
            params![feed_id, dedupe_key],
        )?;
        Ok(())
    }

    pub fn list_unread(&self, limit: usize) -> Result<Vec<ArticleSummary>, Error> {
        let limit = i64::try_from(limit).map_err(|_| Error::message("limit is too large"))?;
        let mut statement = self.connection.prepare(
            "SELECT a.id,a.title,COALESCE(f.title,f.source_url),a.sort_at,a.is_read
             FROM articles a JOIN feeds f ON f.id=a.feed_id
             WHERE a.is_read=0 ORDER BY a.sort_at DESC,a.id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], |row| {
            Ok(ArticleSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                feed_title: row.get(2)?,
                sort_at: row.get(3)?,
                is_read: row.get::<_, i64>(4)? != 0,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn article<'a>(feed_id: i64, key: &'a str, title: &'a str, sort_at: i64) -> ArticleInsert<'a> {
        ArticleInsert {
            feed_id,
            dedupe_key: key,
            guid: Some(key),
            url: Some("https://example.com/article"),
            title,
            author: None,
            published_at: Some(sort_at),
            sort_at,
            fetched_at: sort_at,
            source_kind: 1,
            compression_codec: 1,
            content_format: 1,
            storage_version: 1,
            uncompressed_size: 12,
            content_blob: b"compressed",
        }
    }

    #[test]
    fn migrates_with_conservative_pragmas() {
        let directory = tempfile::tempdir().expect("temp directory");
        let store = Store::open(&directory.path().join("rss.sqlite3")).expect("open store");
        assert_eq!(store.schema_version().expect("version"), SCHEMA_VERSION);
        assert_eq!(store.journal_mode().expect("journal"), "delete");
    }

    #[test]
    fn feed_and_article_inserts_are_idempotent_and_ordered() {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut store = Store::open(&directory.path().join("rss.sqlite3")).expect("open store");
        let feed = store
            .add_feed("https://example.com/feed", 1)
            .expect("add feed");
        assert_eq!(
            feed,
            store
                .add_feed("https://example.com/feed", 2)
                .expect("add again")
        );
        assert!(store
            .insert_article(&article(feed, "a", "Older", 10))
            .expect("insert"));
        assert!(!store
            .insert_article(&article(feed, "a", "Duplicate", 30))
            .expect("dedupe"));
        assert!(store
            .insert_article(&article(feed, "b", "Newer", 20))
            .expect("insert"));
        assert_eq!(store.article_count().expect("count"), 2);
        let unread = store.list_unread(10).expect("list unread");
        assert_eq!(unread[0].title, "Newer");
        assert_eq!(unread[1].title, "Older");
    }

    #[test]
    fn refuses_incomplete_article() {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut store = Store::open(&directory.path().join("rss.sqlite3")).expect("open store");
        let feed = store
            .add_feed("https://example.com/feed", 1)
            .expect("add feed");
        let mut incomplete = article(feed, "a", "Title", 10);
        incomplete.content_blob = b"";
        assert!(store.insert_article(&incomplete).is_err());
        assert_eq!(store.article_count().expect("count"), 0);
    }
}
