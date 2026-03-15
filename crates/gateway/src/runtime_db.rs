use std::path::{Path, PathBuf};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
};
use tracing::debug;

/// Default main SQLite database path under a Moltis data directory.
pub fn default_db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("moltis.db")
}

/// Open the shared main SQLite database used by the gateway and embedded clients.
pub async fn open_sqlite_pool(data_dir: &Path, max_connections: u32) -> anyhow::Result<SqlitePool> {
    use std::str::FromStr;

    let db_path = default_db_path(data_dir);
    let db_exists = db_path.exists();
    let mut options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path.display()))?
        .create_if_missing(true)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5));

    if !db_exists {
        // Setting journal_mode can briefly require an exclusive lock.
        // For existing databases, preserve current mode to avoid startup stalls.
        options = options.journal_mode(SqliteJournalMode::Wal);
    }

    let started = std::time::Instant::now();
    let pool = sqlx::pool::PoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await?;
    debug!(
        path = %db_path.display(),
        db_exists,
        max_connections,
        elapsed_ms = started.elapsed().as_millis(),
        "startup sqlite pool connected"
    );

    Ok(pool)
}

/// Run database migrations for the shared main database in dependency order.
pub async fn migrate_sqlite_pool(pool: &SqlitePool) -> anyhow::Result<()> {
    moltis_projects::run_migrations(pool).await?;
    moltis_sessions::run_migrations(pool).await?;
    moltis_cron::run_migrations(pool).await?;
    crate::run_migrations(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_db_path_is_directory_based() {
        let root = PathBuf::from("/tmp/moltis-data");
        assert_eq!(default_db_path(&root), root.join("moltis.db"));
    }

    #[tokio::test]
    async fn open_sqlite_pool_creates_db_and_runs_migrations() {
        let tmp = tempfile::tempdir().unwrap();
        let pool = open_sqlite_pool(tmp.path(), 4).await.unwrap();
        migrate_sqlite_pool(&pool).await.unwrap();
        pool.close().await;
        assert!(default_db_path(tmp.path()).exists());
    }
}
