use super::App;
use anyhow::{Context, Result, anyhow};
use rusqlite::Connection;
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

impl App {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path).context("opening database")?;
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
          CREATE TABLE IF NOT EXISTS users(id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE COLLATE NOCASE, password_hash TEXT NOT NULL, is_admin INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS sessions(token_hash TEXT PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, expires_at INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS folders(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE);
          CREATE TABLE IF NOT EXISTS folder_sync(folder_id INTEGER PRIMARY KEY REFERENCES folders(id) ON DELETE CASCADE, report_json TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS export_templates(format TEXT PRIMARY KEY, source TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS recipes(id INTEGER PRIMARY KEY AUTOINCREMENT, folder_id INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE, path TEXT NOT NULL UNIQUE, title TEXT NOT NULL, raw_toml TEXT NOT NULL, data_json TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS recipes_title ON recipes(title COLLATE NOCASE, id);")?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            sync_lock: Arc::new(Mutex::new(())),
        })
    }
    pub(super) fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|_| anyhow!("database lock poisoned"))
    }
}

pub(super) fn now() -> Result<i64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64)
}
