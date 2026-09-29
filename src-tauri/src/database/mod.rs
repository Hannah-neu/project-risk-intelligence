pub(crate) mod migrations;

use chrono::Local;
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::Duration,
};
use thiserror::Error;

pub const DATABASE_FILE_NAME: &str = "project-risk.db";

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("failed to prepare the application data directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("database operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database connection lock is unavailable")]
    ConnectionLock,
}

pub struct Database {
    connection: Mutex<Connection>,
    path: PathBuf,
}

impl Database {
    pub fn initialize(app_data_dir: &Path) -> Result<Self, DatabaseError> {
        fs::create_dir_all(app_data_dir)?;
        let path = app_data_dir.join(DATABASE_FILE_NAME);
        let mut connection = Connection::open(&path)?;

        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.busy_timeout(Duration::from_secs(5))?;
        migrations::run(&mut connection)?;
        refresh_derived_data_if_stale(&mut connection)?;

        Ok(Self {
            connection: Mutex::new(connection),
            path,
        })
    }

    pub fn connection(&self) -> Result<MutexGuard<'_, Connection>, DatabaseError> {
        self.connection
            .lock()
            .map_err(|_| DatabaseError::ConnectionLock)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn size_bytes(&self) -> Result<u64, DatabaseError> {
        Ok(fs::metadata(&self.path)?.len())
    }
}

fn refresh_derived_data_if_stale(connection: &mut Connection) -> Result<bool, DatabaseError> {
    let today = Local::now().date_naive();
    if crate::risk_engine::cache_is_current(connection, today)? {
        return Ok(false);
    }

    let transaction = connection.transaction()?;
    crate::risk_engine::recalculate_all(&transaction)?;
    crate::insight_engine::recalculate_all(&transaction)?;
    crate::risk_engine::mark_cache_current(&transaction, today)?;
    transaction.commit()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_data_is_reused_until_the_cache_becomes_incomplete() {
        let mut connection = Connection::open_in_memory().expect("database should open");
        migrations::run(&mut connection).expect("migrations should run");

        assert!(refresh_derived_data_if_stale(&mut connection).expect("first refresh should run"));
        assert!(
            !refresh_derived_data_if_stale(&mut connection).expect("fresh cache should be reused")
        );

        connection
            .execute("DELETE FROM ai_insights", [])
            .expect("test should invalidate cached insights");
        assert!(refresh_derived_data_if_stale(&mut connection)
            .expect("incomplete cache should be rebuilt"));

        crate::repositories::settings::upsert(
            &connection,
            "last_risk_calculated_date",
            "2000-01-01",
        )
        .expect("test should invalidate the calculation date");
        assert!(refresh_derived_data_if_stale(&mut connection)
            .expect("a new local date should rebuild the cache"));

        crate::repositories::settings::upsert(&connection, "risk_rule_version", "outdated")
            .expect("test should invalidate the rule version");
        assert!(refresh_derived_data_if_stale(&mut connection)
            .expect("a new risk rule version should rebuild the cache"));
    }
}
