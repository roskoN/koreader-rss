//! Minimal SQLite database shared with the KOReader Lua probe.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use crate::Error;

pub const META_KEY_PROBE: &str = "probe";
pub const META_VALUE_PROBE: &str = "milestone0";

fn absolute(path: &Path) -> Result<PathBuf, Error> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

pub fn initialize(path: &Path) -> Result<PathBuf, Error> {
    let path = absolute(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let connection = Connection::open(&path)?;
    connection.execute_batch(
        "PRAGMA journal_mode=DELETE;
         PRAGMA synchronous=FULL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS meta (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );",
    )?;
    connection.execute(
        "INSERT OR REPLACE INTO meta(key,value) VALUES (?1,?2)",
        params![META_KEY_PROBE, META_VALUE_PROBE],
    )?;
    Ok(path)
}

pub fn meta_value(connection: &Connection, key: &str) -> Result<Option<String>, Error> {
    Ok(connection
        .query_row("SELECT value FROM meta WHERE key=?1", params![key], |row| {
            row.get(0)
        })
        .optional()?)
}

pub fn probe_present(path: &Path) -> Result<bool, Error> {
    let connection = Connection::open(path)?;
    Ok(meta_value(&connection, META_KEY_PROBE)?.as_deref() == Some(META_VALUE_PROBE))
}

pub fn sqlite_version(path: &Path) -> Result<String, Error> {
    let connection = Connection::open(path)?;
    Ok(connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?)
}

pub fn journal_mode(path: &Path) -> Result<String, Error> {
    let connection = Connection::open(path)?;
    Ok(connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_old_compatible_probe_database() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("probe.sqlite3");
        initialize(&path).expect("initialize probe DB");
        assert!(probe_present(&path).expect("query probe"));
        assert_eq!(journal_mode(&path).expect("journal mode"), "delete");
        assert!(!sqlite_version(&path).expect("SQLite version").is_empty());
    }
}
