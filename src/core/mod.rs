use rusqlite::Connection;
use std::sync::{Arc, Mutex};

mod accounts;
mod models;
mod parsing;
mod recipes;
mod sessions;
mod storage;
mod sync;
mod templates;
#[cfg(test)]
mod tests;

pub use models::*;

pub const TOKEN_TTL: i64 = 30 * 24 * 60 * 60;
const MAX_RECIPE_BYTES: u64 = 1024 * 1024;

#[derive(Clone)]
pub struct App {
    db: Arc<Mutex<Connection>>,
    sync_lock: Arc<Mutex<()>>,
}
