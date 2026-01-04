use anyhow::{Context, Result};
use redb::{Database, ReadableTable, TableDefinition};
use std::path::PathBuf;
use trade_shared::{PersistedPanicStopState, Side};

pub const PANIC_STOPS: TableDefinition<&str, &[u8]> = TableDefinition::new("panic_stops");

fn make_key(state: &PersistedPanicStopState) -> String {
    make_key_from_parts(&state.symbol, state.side)
}

fn make_key_from_parts(symbol: &str, side: Side) -> String {
    format!("{}:{:?}", symbol, side)
}

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

pub fn save_panic_stop(db: &Database, state: &PersistedPanicStopState) -> Result<()> {
    let key = make_key(state);
    let serialized = bincode::serialize(state).context("Failed to serialize PanicStopState")?;

    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(PANIC_STOPS)?;
        table.insert(key.as_str(), serialized.as_slice())?;
    }
    write_txn.commit()?;

    tracing::debug!("Saved panic stop state for {}", key);
    Ok(())
}

pub fn load_panic_stop(
    db: &Database,
    symbol: &str,
    side: Side,
) -> Result<Option<PersistedPanicStopState>> {
    let key = make_key_from_parts(symbol, side);

    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(PANIC_STOPS)?;

    match table.get(key.as_str())? {
        Some(value) => {
            let state: PersistedPanicStopState = bincode::deserialize(value.value())
                .context("Failed to deserialize PanicStopState")?;
            tracing::debug!("Loaded panic stop state for {}", key);
            Ok(Some(state))
        }
        None => {
            tracing::debug!("No panic stop state found for {}", key);
            Ok(None)
        }
    }
}
