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
    pub schedule_order: i64,
    pub content_selector: Option<String>,
    pub remove_selector: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FeedSummary {
    pub id: i64,
    pub source_url: String,
    pub title: Option<String>,
    pub enabled: bool,
    pub failure_count: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RefreshRunSummary {
    pub id: i64,
    pub started_at: i64,
    pub finished_at: i64,
    pub feeds_checked: i64,
    pub new_articles: i64,
    pub failed_feeds: i64,
    pub budget_s: i64,
    pub reason: i64,
    pub outcome: Option<i64>,
    pub last_error: Option<String>,
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
    pub url: Option<String>,
}

#[derive(Debug)]
pub struct ArticleContent {
    pub storage_version: i64,
    pub fetched_at: i64,
    pub uncompressed_size: usize,
    pub compression_codec: i64,
    pub content_blob: Vec<u8>,
}

pub struct Store {
    connection: Connection,
}

pub const RUN_REASON_MANUAL: i64 = 1;
pub const RUN_OUTCOME_SUCCESS: i64 = 1;
pub const RUN_OUTCOME_PARTIAL: i64 = 2;
pub const RUN_OUTCOME_INTERRUPTED: i64 = 3;
pub const RUN_OUTCOME_FAILED: i64 = 4;

fn backoff_delay(source_url: &str, failures: i64) -> i64 {
    let base = match failures {
        0 | 1 => 900,
        2 => 3_600,
        3 => 14_400,
        4 => 43_200,
        _ => 86_400,
    };
    let mut hash = 2_166_136_261u32;
    for byte in source_url.bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(16_777_619);
    }
    let percent = 90 + i64::from(hash % 21);
    base * percent / 100
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
        self.add_feed_with_title(url, None, now)
    }

    pub fn add_feed_with_title(
        &mut self,
        url: &str,
        title: Option<&str>,
        now: i64,
    ) -> Result<i64, Error> {
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
            "INSERT OR IGNORE INTO feeds(source_url,title,schedule_order,created_at) VALUES (?1,?2,?3,?4)",
            params![url, title, schedule_order, now],
        )?;
        if let Some(title) = title {
            transaction.execute(
                "UPDATE feeds SET title=?2 WHERE source_url=?1 AND (title IS NULL OR title='')",
                params![url, title],
            )?;
        }
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
            "SELECT id,source_url,effective_url,title,http_etag,http_last_modified,next_due_at,schedule_order,content_selector,remove_selector
             FROM feeds WHERE id=?1 AND enabled=1"
        } else {
            "SELECT id,source_url,effective_url,title,http_etag,http_last_modified,next_due_at,schedule_order,content_selector,remove_selector
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

    pub fn feed_ids(&self) -> Result<Vec<i64>, Error> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM feeds WHERE enabled=1 ORDER BY schedule_order")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn list_feeds(&self) -> Result<Vec<FeedSummary>, Error> {
        let mut statement = self
            .connection
            .prepare("SELECT id,source_url,title,enabled,failure_count,last_error FROM feeds ORDER BY schedule_order")?;
        let rows = statement.query_map([], |row| {
            Ok(FeedSummary {
                id: row.get(0)?,
                source_url: row.get(1)?,
                title: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                failure_count: row.get(4)?,
                last_error: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn remove_feed(&mut self, id: i64) -> Result<bool, Error> {
        Ok(self
            .connection
            .execute("DELETE FROM feeds WHERE id=?1", params![id])?
            == 1)
    }

    pub fn remove_all_feeds(&mut self) -> Result<usize, Error> {
        Ok(self.connection.execute("DELETE FROM feeds", [])?)
    }

    pub fn remove_all_articles(&mut self) -> Result<usize, Error> {
        Ok(self.connection.execute("DELETE FROM articles", [])?)
    }

    pub fn set_feed_enabled(&mut self, id: i64, enabled: bool) -> Result<bool, Error> {
        Ok(self.connection.execute(
            "UPDATE feeds SET enabled=?2 WHERE id=?1",
            params![id, i64::from(enabled)],
        )? == 1)
    }

    pub fn set_feed_selectors(
        &mut self,
        id: i64,
        content: Option<&str>,
        remove: Option<&str>,
    ) -> Result<bool, Error> {
        Ok(self.connection.execute(
            "UPDATE feeds SET content_selector=?2,remove_selector=?3 WHERE id=?1",
            params![id, content, remove],
        )? == 1)
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
            schedule_order: row.get(7)?,
            content_selector: row.get(8)?,
            remove_selector: row.get(9)?,
        })
    }

    pub fn due_feeds(&self, now: i64) -> Result<Vec<i64>, Error> {
        let cursor: i64 = self.connection.query_row(
            "SELECT scheduler_cursor FROM app_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let mut statement = self.connection.prepare(
            "SELECT id,schedule_order FROM feeds WHERE enabled=1 AND next_due_at<=?1 ORDER BY schedule_order",
        )?;
        let rows = statement.query_map(params![now], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut feeds = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        let split = feeds
            .iter()
            .position(|(_, order)| *order >= cursor)
            .unwrap_or(0);
        feeds.rotate_left(split);
        Ok(feeds.into_iter().map(|(id, _)| id).collect())
    }

    pub fn begin_refresh_run(
        &mut self,
        reason: i64,
        budget_s: u64,
        now: i64,
    ) -> Result<i64, Error> {
        let budget_s =
            i64::try_from(budget_s).map_err(|_| Error::message("budget is too large"))?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE refresh_runs SET finished_at=?1,outcome=?2,last_error='interrupted' WHERE finished_at IS NULL", params![now, RUN_OUTCOME_INTERRUPTED])?;
        tx.execute(
            "INSERT INTO refresh_runs(reason,started_at,budget_s) VALUES (?1,?2,?3)",
            params![reason, now, budget_s],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    pub fn record_feed_attempt(
        &mut self,
        run_id: i64,
        feed: &FeedRow,
        inserted: usize,
        failed: usize,
    ) -> Result<(), Error> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE app_state SET scheduler_cursor=?1 WHERE singleton=1",
            params![feed.schedule_order + 1],
        )?;
        tx.execute("UPDATE refresh_runs SET feeds_checked=feeds_checked+1,new_articles=new_articles+?2,failed_feeds=failed_feeds+CASE WHEN ?3>0 THEN 1 ELSE 0 END WHERE id=?1", params![run_id, i64::try_from(inserted).unwrap_or(i64::MAX), i64::try_from(failed).unwrap_or(i64::MAX)])?;
        tx.commit()?;
        Ok(())
    }

    pub fn finish_refresh_run(
        &mut self,
        id: i64,
        now: i64,
        outcome: i64,
        error: Option<&str>,
    ) -> Result<(), Error> {
        self.connection.execute(
            "UPDATE refresh_runs SET finished_at=?2,outcome=?3,last_error=?4 WHERE id=?1",
            params![id, now, outcome, error],
        )?;
        self.connection.execute(
            "DELETE FROM refresh_runs WHERE id NOT IN (SELECT id FROM refresh_runs ORDER BY started_at DESC,id DESC LIMIT 20)",
            [],
        )?;
        Ok(())
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

    pub fn article_content(&self, id: i64) -> Result<Option<ArticleContent>, Error> {
        Ok(self.connection.query_row(
            "SELECT id,storage_version,fetched_at,uncompressed_size,compression_codec,content_blob FROM articles WHERE id=?1",
            params![id],
            |row| Ok(ArticleContent {
                storage_version: row.get(1)?, fetched_at: row.get(2)?,
                uncompressed_size: row.get::<_, i64>(3)?.try_into().unwrap_or(0),
                compression_codec: row.get(4)?, content_blob: row.get(5)?,
            }),
        ).optional()?)
    }

    pub fn mark_read(&mut self, id: i64, read: bool, now: i64) -> Result<bool, Error> {
        let changed = self.connection.execute(
            "UPDATE articles SET is_read=?2,read_at=CASE WHEN ?2=1 THEN ?3 ELSE NULL END WHERE id=?1",
            params![id, i64::from(read), now],
        )?;
        Ok(changed == 1)
    }

    pub fn prune(&mut self, now: i64) -> Result<usize, Error> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (retention_days, max_per_feed, max_total, max_db_bytes): (i64, i64, i64, i64) = tx.query_row(
            "SELECT retention_days,max_articles_per_feed,max_articles_total,max_db_bytes FROM settings WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        let age = now - retention_days.saturating_mul(86_400);
        let mut removed = 0usize;
        loop {
            let count = tx.execute(
                "DELETE FROM articles WHERE id IN (SELECT id FROM articles WHERE is_read=1 AND sort_at<?1 ORDER BY sort_at,id LIMIT 50)",
                params![age],
            )?;
            removed += count;
            if count == 0 {
                break;
            }
        }
        let total: i64 = tx.query_row("SELECT count(*) FROM articles", [], |row| row.get(0))?;
        if total > max_total {
            tx.execute("DELETE FROM articles WHERE id IN (SELECT id FROM articles WHERE is_read=1 ORDER BY sort_at,id LIMIT ?1)", params![total - max_total])?;
        }
        tx.execute(
            "DELETE FROM articles WHERE id IN (
                SELECT a.id FROM articles a JOIN (
                    SELECT feed_id,COUNT(*) AS count FROM articles GROUP BY feed_id HAVING count > ?1
                ) excess ON excess.feed_id=a.feed_id
                WHERE a.is_read=1 ORDER BY a.sort_at,a.id
            )",
            params![max_per_feed],
        )?;
        if max_db_bytes > 0 {
            loop {
                let logical: i64 = tx.query_row(
                    "SELECT COALESCE(SUM(compressed_size),0) FROM articles",
                    [],
                    |row| row.get(0),
                )?;
                if logical <= max_db_bytes {
                    break;
                }
                let removed = tx.execute("DELETE FROM articles WHERE id IN (SELECT id FROM articles WHERE is_read=1 ORDER BY sort_at,id LIMIT 50)", [])?;
                if removed == 0 {
                    break;
                }
            }
        }
        tx.commit()?;
        self.maintain_freelist()?;
        self.enforce_physical_limit(max_db_bytes)?;
        Ok(removed)
    }

    fn enforce_physical_limit(&mut self, limit: i64) -> Result<(), Error> {
        if limit <= 0 {
            return Ok(());
        }
        for _ in 0..128 {
            if self.database_bytes()? <= limit {
                break;
            }
            let removed = self.connection.execute(
                "DELETE FROM articles WHERE id IN (SELECT id FROM articles WHERE is_read=1 ORDER BY sort_at,id LIMIT 50)",
                [],
            )?;
            if removed == 0 {
                break;
            }
            self.maintain_freelist()?;
        }
        Ok(())
    }

    fn maintain_freelist(&mut self) -> Result<(), Error> {
        let free: i64 = self
            .connection
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
        if free > 0 {
            let pages = free.min(128);
            self.connection
                .execute_batch(&format!("PRAGMA incremental_vacuum({pages})"))?;
        }
        Ok(())
    }

    pub fn database_bytes(&self) -> Result<i64, Error> {
        let pages: i64 = self
            .connection
            .query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let page_size: i64 = self
            .connection
            .query_row("PRAGMA page_size", [], |row| row.get(0))?;
        Ok(pages.saturating_mul(page_size))
    }

    pub fn freelist_pages(&self) -> Result<i64, Error> {
        Ok(self
            .connection
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))?)
    }

    pub fn latest_refresh_run(&self) -> Result<Option<RefreshRunSummary>, Error> {
        self.connection
            .query_row(
                "SELECT id,started_at,COALESCE(finished_at,0),feeds_checked,new_articles,failed_feeds,budget_s,reason,outcome,last_error FROM refresh_runs ORDER BY started_at DESC,id DESC LIMIT 1",
                [],
                |row| Ok(RefreshRunSummary {
                    id: row.get(0)?, started_at: row.get(1)?, finished_at: row.get(2)?,
                    feeds_checked: row.get(3)?, new_articles: row.get(4)?, failed_feeds: row.get(5)?,
                    budget_s: row.get(6)?, reason: row.get(7)?, outcome: row.get(8)?, last_error: row.get(9)?,
                }),
            )
            .optional()
            .map_err(Error::from)
    }

    pub fn list_articles(
        &self,
        limit: usize,
        unread_only: bool,
    ) -> Result<Vec<ArticleSummary>, Error> {
        let limit = i64::try_from(limit).map_err(|_| Error::message("limit is too large"))?;
        let filter = if unread_only { "WHERE a.is_read=0" } else { "" };
        let sql = format!("SELECT a.id,a.title,COALESCE(f.title,f.source_url),a.sort_at,a.is_read,a.url FROM articles a JOIN feeds f ON f.id=a.feed_id {filter} ORDER BY a.sort_at DESC,a.id DESC LIMIT ?1");
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(params![limit], |row| {
            Ok(ArticleSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                feed_title: row.get(2)?,
                sort_at: row.get(3)?,
                is_read: row.get::<_, i64>(4)? != 0,
                url: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn update_feed_failure(
        &mut self,
        feed_id: i64,
        error: &str,
        now: i64,
    ) -> Result<(), Error> {
        self.update_feed_failure_with_retry(feed_id, error, now, None)
    }

    pub fn update_feed_failure_with_retry(
        &mut self,
        feed_id: i64,
        error: &str,
        now: i64,
        retry_after_s: Option<i64>,
    ) -> Result<(), Error> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (source_url, failures): (String, i64) = tx.query_row(
            "SELECT source_url,failure_count+1 FROM feeds WHERE id=?1",
            params![feed_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let delay = retry_after_s
            .unwrap_or_else(|| backoff_delay(&source_url, failures))
            .clamp(0, 48 * 60 * 60);
        tx.execute(
            "UPDATE feeds SET last_attempt_at=?2,last_checked_at=?2,failure_count=?4,last_error=?3,backoff_until=?2+?5,next_due_at=?2+?5 WHERE id=?1",
            params![feed_id, now, error, failures, delay],
        )?;
        tx.commit()?;
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
            "SELECT a.id,a.title,COALESCE(f.title,f.source_url),a.sort_at,a.is_read,a.url
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
                url: row.get(5)?,
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

    #[test]
    fn due_feeds_rotate_from_persisted_cursor_and_run_is_recorded() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("rss.sqlite3");
        let mut store = Store::open(&path).expect("open store");
        let first = store.add_feed("https://example.com/1", 1).expect("feed");
        let second = store.add_feed("https://example.com/2", 1).expect("feed");
        let third = store.add_feed("https://example.com/3", 1).expect("feed");
        assert_eq!(store.due_feeds(1).expect("due"), vec![first, second, third]);
        let run = store
            .begin_refresh_run(RUN_REASON_MANUAL, 60, 10)
            .expect("run");
        let feed = store.feed(Some(first)).expect("lookup").expect("feed");
        store
            .record_feed_attempt(run, &feed, 2, 0)
            .expect("attempt");
        assert_eq!(store.due_feeds(1).expect("due"), vec![second, third, first]);
        store
            .finish_refresh_run(run, 11, RUN_OUTCOME_SUCCESS, None)
            .expect("finish");
        let (finished, outcome, articles): (Option<i64>, i64, i64) = store
            .connection
            .query_row(
                "SELECT finished_at,outcome,new_articles FROM refresh_runs WHERE id=?1",
                params![run],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("run row");
        assert_eq!(finished, Some(11));
        assert_eq!(outcome, RUN_OUTCOME_SUCCESS);
        assert_eq!(articles, 2);
        assert_eq!(store.due_feeds(1).expect("due"), vec![second, third, first]);
    }

    #[test]
    fn starting_run_marks_previous_run_interrupted() {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut store = Store::open(&directory.path().join("rss.sqlite3")).expect("open store");
        let old = store
            .begin_refresh_run(RUN_REASON_MANUAL, 60, 10)
            .expect("run");
        let new = store
            .begin_refresh_run(RUN_REASON_MANUAL, 60, 20)
            .expect("run");
        let old_outcome: i64 = store
            .connection
            .query_row(
                "SELECT outcome FROM refresh_runs WHERE id=?1",
                params![old],
                |row| row.get(0),
            )
            .expect("old run");
        assert_eq!(old_outcome, RUN_OUTCOME_INTERRUPTED);
        assert!(new > old);
    }

    #[test]
    fn feed_failures_use_bounded_stable_backoff_jitter() {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut store = Store::open(&directory.path().join("rss.sqlite3")).expect("open store");
        let feed = store.add_feed("https://example.com/feed", 1).expect("feed");
        store
            .update_feed_failure(feed, "offline", 100)
            .expect("failure");
        let first = store.feed(Some(feed)).expect("lookup").expect("feed");
        assert!(first.next_due_at > 100 + 800);
        assert!(first.next_due_at < 100 + 1_000);
        store
            .update_feed_failure(feed, "offline", 200)
            .expect("failure");
        let second = store.feed(Some(feed)).expect("lookup").expect("feed");
        assert!(second.next_due_at > 200 + 3_200);
        assert!(second.next_due_at < 200 + 4_000);
        assert_eq!(
            backoff_delay("https://example.com/feed", 2),
            second.next_due_at - 200
        );
    }

    #[test]
    fn database_bytes_reports_physical_page_size() {
        let directory = tempfile::tempdir().expect("temp directory");
        let store = Store::open(&directory.path().join("rss.sqlite3")).expect("open store");
        assert!(store.database_bytes().expect("size") > 0);
    }

    #[test]
    fn retention_prefers_removing_read_articles() {
        let directory = tempfile::tempdir().expect("temp directory");
        let mut store = Store::open(&directory.path().join("rss.sqlite3")).expect("open store");
        let feed = store.add_feed("https://example.org/feed", 1).expect("feed");
        store
            .insert_article(&article(feed, "read", "Read", 1))
            .expect("article");
        store
            .insert_article(&article(feed, "unread", "Unread", 2))
            .expect("article");
        store.mark_read(1, true, 3).expect("mark read");
        store
            .connection
            .execute("UPDATE settings SET max_articles_total=1", [])
            .expect("setting");
        store.prune(100).expect("prune");
        assert_eq!(store.article_count().expect("count"), 1);
        assert_eq!(store.list_unread(10).expect("unread")[0].title, "Unread");
    }
}
