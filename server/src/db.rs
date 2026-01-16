use anyhow::{Context, Result};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::path::PathBuf;
use trade_shared::{PersistedActivityStopState, PendingAutostopCommand, Side};

pub const ACTIVITY_STOPS: TableDefinition<&str, &[u8]> = TableDefinition::new("activity_stops");
pub const PENDING_AUTOSTOPS: TableDefinition<&str, &[u8]> = TableDefinition::new("pending_autostops");

fn make_key(state: &PersistedActivityStopState) -> String {
    make_key_from_parts(&state.symbol.0, state.side)
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
        let _ = write_txn.open_table(ACTIVITY_STOPS)?;
        let _ = write_txn.open_table(PENDING_AUTOSTOPS)?;
    }
    write_txn.commit()?;

    Ok(db)
}

pub fn save_activity_stop(db: &Database, state: &PersistedActivityStopState) -> Result<()> {
    let key = make_key(state);
    let serialized = serde_json::to_vec(state).context("Failed to serialize PanicStopState")?;

    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(ACTIVITY_STOPS)?;
        table.insert(key.as_str(), serialized.as_slice())?;
    }
    write_txn.commit()?;

    tracing::debug!("Saved panic stop state for {}", key);
    Ok(())
}

#[allow(dead_code)]
pub fn load_activity_stop(
    db: &Database,
    symbol: &str,
    side: Side,
) -> Result<Option<PersistedActivityStopState>> {
    let key = make_key_from_parts(symbol, side);

    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(ACTIVITY_STOPS)?;

    match table.get(key.as_str())? {
        Some(value) => {
            let state: PersistedActivityStopState = serde_json::from_slice(value.value())
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

pub fn delete_activity_stop(db: &Database, symbol: &str, side: Side) -> Result<()> {
    let key = make_key_from_parts(symbol, side);

    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(ACTIVITY_STOPS)?;
        table.remove(key.as_str())?;
    }
    write_txn.commit()?;

    tracing::debug!("Deleted panic stop state for {}", key);
    Ok(())
}

pub fn load_all_activity_stops(db: &Database) -> Result<Vec<PersistedActivityStopState>> {
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(ACTIVITY_STOPS)?;

    let mut states = Vec::new();

    for entry in table.iter()? {
        let (key, value) = entry?;
        match serde_json::from_slice::<PersistedActivityStopState>(value.value()) {
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

fn make_pending_key(limit_order_id: &str) -> String {
    format!("pending:{}", limit_order_id)
}

pub fn save_pending_autostop(db: &Database, cmd: &PendingAutostopCommand) -> Result<()> {
    let key = make_pending_key(&cmd.limit_order_id);
    let serialized = serde_json::to_vec(cmd)
        .context("Failed to serialize PendingAutostopCommand")?;

    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(PENDING_AUTOSTOPS)?;
        table.insert(key.as_str(), serialized.as_slice())?;
    }
    write_txn.commit()?;

    tracing::debug!("Saved pending autostop for order {}", cmd.limit_order_id);
    Ok(())
}

pub fn load_pending_autostop(
    db: &Database,
    limit_order_id: &str,
) -> Result<Option<PendingAutostopCommand>> {
    let key = make_pending_key(limit_order_id);

    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(PENDING_AUTOSTOPS)?;

    match table.get(key.as_str())? {
        Some(value) => {
            let cmd: PendingAutostopCommand = serde_json::from_slice(value.value())
                .context("Failed to deserialize PendingAutostopCommand")?;
            tracing::debug!("Loaded pending autostop for order {}", limit_order_id);
            Ok(Some(cmd))
        }
        None => {
            tracing::debug!("No pending autostop found for order {}", limit_order_id);
            Ok(None)
        }
    }
}

pub fn delete_pending_autostop(db: &Database, limit_order_id: &str) -> Result<()> {
    let key = make_pending_key(limit_order_id);

    let write_txn = db.begin_write()?;
    {
        let mut table = write_txn.open_table(PENDING_AUTOSTOPS)?;
        table.remove(key.as_str())?;
    }
    write_txn.commit()?;

    tracing::debug!("Deleted pending autostop for order {}", limit_order_id);
    Ok(())
}

pub fn load_all_pending_autostops(db: &Database) -> Result<Vec<PendingAutostopCommand>> {
    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(PENDING_AUTOSTOPS)?;

    let mut commands = Vec::new();

    for entry in table.iter()? {
        let (key, value) = entry?;
        match serde_json::from_slice::<PendingAutostopCommand>(value.value()) {
            Ok(cmd) => {
                tracing::debug!("Loaded pending autostop for {}", key.value());
                commands.push(cmd);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to deserialize pending autostop for {}: {}",
                    key.value(),
                    e
                );
            }
        }
    }

    tracing::info!("Loaded {} pending autostops from database", commands.len());
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use trade_shared::{Symbol, Side};
    use rust_decimal_macros::dec;

    fn create_test_db() -> (tempfile::TempDir, Database) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("data.redb");
        let db = Database::create(&db_path).unwrap();
        let write_txn = db.begin_write().unwrap();
        {
            let _ = write_txn.open_table(ACTIVITY_STOPS).unwrap();
        }
        write_txn.commit().unwrap();
        (dir, db)
    }

    fn create_test_state() -> PersistedActivityStopState {
        PersistedActivityStopState {
            symbol: Symbol::new("BTCUSDT"),
            side: Side::Buy,
            timeout_ms: 5000,
            trigger_price: Some(dec!(95000)),
            start_timestamp: 1704384000000,
            active: true,
        }
    }

    #[test]
    fn test_save_activity_stop() {
        let (_dir, db) = create_test_db();
        let state = create_test_state();
        assert!(save_activity_stop(&db, &state).is_ok());
    }

    #[test]
    fn test_load_activity_stop() {
        let (_dir, db) = create_test_db();
        let state = create_test_state();
        save_activity_stop(&db, &state).unwrap();

        let loaded = load_activity_stop(&db, "BTCUSDT", Side::Buy).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.symbol.0, "BTCUSDT");
        assert_eq!(loaded.timeout_ms, 5000);
    }

    #[test]
    fn test_delete_activity_stop() {
        let (_dir, db) = create_test_db();
        let state = create_test_state();
        save_activity_stop(&db, &state).unwrap();
        delete_activity_stop(&db, "BTCUSDT", Side::Buy).unwrap();

        let loaded = load_activity_stop(&db, "BTCUSDT", Side::Buy).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_load_nonexistent() {
        let (_dir, db) = create_test_db();
        let loaded = load_activity_stop(&db, "NONEXISTENT", Side::Buy).unwrap();
        assert!(loaded.is_none());
    }
}
