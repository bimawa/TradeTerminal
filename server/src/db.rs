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

pub fn delete_panic_stop(db: &Database, symbol: &str, side: Side) -> Result<()> {
    let key = make_key_from_parts(symbol, side);

    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(PANIC_STOPS)?;
        table.remove(key.as_str())?;
    }
    write_txn.commit()?;

    tracing::debug!("Deleted panic stop state for {}", key);
    Ok(())
}

pub fn load_all_panic_stops(db: &Database) -> Result<Vec<PersistedPanicStopState>> {
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(PANIC_STOPS)?;

    let mut states = Vec::new();

    for entry in table.iter()? {
        let (key, value) = entry?;
        match bincode::deserialize::<PersistedPanicStopState>(value.value()) {
            Ok(state) => {
                tracing::debug!("Loaded panic stop state for {}", key.value());
                states.push(state);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to deserialize panic stop state for {}: {}",
                    key.value(),
                    e
                );
            }
        }
    }

    tracing::info!("Loaded {} panic stop states from database", states.len());
    Ok(states)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use trade_shared::{Symbol, Side};
    use rust_decimal_macros::dec;

    fn create_test_db() -> (tempfile::TempDir, Database) {
        let dir = tempdir().unwrap();
        std::env::set_var("DATA_DIR", dir.path());
        let db = init_database().unwrap();
        (dir, db)
    }

    fn create_test_state() -> PersistedPanicStopState {
        PersistedPanicStopState {
            symbol: Symbol::new("BTCUSDT"),
            side: Side::Buy,
            timeout_ms: 5000,
            trigger_price: Some(dec!(95000)),
            start_timestamp: 1704384000000,
            active: true,
        }
    }

    #[test]
    fn test_save_panic_stop() {
        let (_dir, db) = create_test_db();
        let state = create_test_state();
        assert!(save_panic_stop(&db, &state).is_ok());
    }

    #[test]
    fn test_load_panic_stop() {
        let (_dir, db) = create_test_db();
        let state = create_test_state();
        save_panic_stop(&db, &state).unwrap();

        let loaded = load_panic_stop(&db, "BTCUSDT", Side::Buy).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.symbol.0, "BTCUSDT");
        assert_eq!(loaded.timeout_ms, 5000);
    }

    #[test]
    fn test_delete_panic_stop() {
        let (_dir, db) = create_test_db();
        let state = create_test_state();
        save_panic_stop(&db, &state).unwrap();
        delete_panic_stop(&db, "BTCUSDT", Side::Buy).unwrap();

        let loaded = load_panic_stop(&db, "BTCUSDT", Side::Buy).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_load_nonexistent() {
        let (_dir, db) = create_test_db();
        let loaded = load_panic_stop(&db, "NONEXISTENT", Side::Buy).unwrap();
        assert!(loaded.is_none());
    }
}
