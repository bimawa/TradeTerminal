use anyhow::{Context, Result};
use redb::{Database, TableDefinition};
use std::path::PathBuf;

pub const PANIC_STOPS: TableDefinition<&str, &[u8]> = TableDefinition::new("panic_stops");

pub fn get_data_dir() -> PathBuf {
    std::env::var("DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("trade-terminal")
        })
}

pub fn init_database() -> Result<Database> {
    let data_dir = get_data_dir();
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("Failed to create data directory: {}", data_dir.display()))?;

    let db_path = data_dir.join("data.redb");
    tracing::info!("Opening database at {}", db_path.display());

    let db = Database::create(&db_path)
        .with_context(|| format!("Failed to create database at {}", db_path.display()))?;

    let write_txn = db.begin_write()?;
    {
        let _ = write_txn.open_table(PANIC_STOPS)?;
    }
    write_txn.commit()?;

    Ok(db)
}
