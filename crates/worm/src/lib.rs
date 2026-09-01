//! CRYPTEK VIGIL — WORM Chain (Write Once Read Many)
//!
//! Append-only SHA-256 hash-chained audit trail backed by SQLite.
//! Audit_Spec: 4b565498-9afc-4782-af4a-c6b11a5d0058

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WormBlock {
    pub idx: u64,
    pub timestamp: DateTime<Utc>,
    pub prev_hash: String,
    pub block_type: String,
    pub data: serde_json::Value,
    pub hash: String,
}

pub fn init_worm_db(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS worm_chain (
            idx        INTEGER PRIMARY KEY,
            timestamp  TEXT NOT NULL,
            prev_hash  TEXT NOT NULL,
            block_type TEXT NOT NULL,
            data       TEXT NOT NULL,
            hash       TEXT NOT NULL
        );",
    )?;
    Ok(())
}

fn compute_hash(idx: u64, prev_hash: &str, block_type: &str, data: &serde_json::Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(idx.to_string().as_bytes());
    hasher.update(prev_hash.as_bytes());
    hasher.update(block_type.as_bytes());
    hasher.update(serde_json::to_vec(data).unwrap_or_default());
    format!("{:x}", hasher.finalize())
}

pub fn seed_genesis(conn: &Connection) -> Result<(), rusqlite::Error> {
    let count: u64 = conn.query_row("SELECT COUNT(*) FROM worm_chain", [], |row| row.get(0))?;
    if count > 0 {
        return Ok(());
    }
    let data = serde_json::json!({
        "platform": "CRYPTEK VIGIL",
        "version": "2.0.0",
        "audit_spec": "4b565498-9afc-4782-af4a-c6b11a5d0058",
        "purpose": "Dynasty Wealth Monitoring Protocol"
    });
    let hash = compute_hash(0, "", "genesis", &data);
    let now = Utc::now();
    conn.execute(
        "INSERT INTO worm_chain (idx, timestamp, prev_hash, block_type, data, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![0i64, now.to_rfc3339(), "", "genesis", serde_json::to_string(&data).unwrap(), hash],
    )?;
    Ok(())
}

pub fn append(conn: &Connection, block_type: &str, data: serde_json::Value) -> Result<WormBlock, rusqlite::Error> {
    let (last_idx, last_hash): (u64, String) = conn
        .query_row("SELECT idx, hash FROM worm_chain ORDER BY idx DESC LIMIT 1", [], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap_or((0, String::new()));
    let idx = last_idx + 1;
    let hash = compute_hash(idx, &last_hash, block_type, &data);
    let now = Utc::now();
    conn.execute(
        "INSERT INTO worm_chain (idx, timestamp, prev_hash, block_type, data, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![idx as i64, now.to_rfc3339(), last_hash, block_type, serde_json::to_string(&data).unwrap(), hash],
    )?;
    Ok(WormBlock { idx, timestamp: now, prev_hash: last_hash, block_type: block_type.to_string(), data, hash })
}

pub fn verify(conn: &Connection) -> Result<(bool, u64), rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT idx, timestamp, prev_hash, block_type, data, hash FROM worm_chain ORDER BY idx")?;
    let blocks: Vec<WormBlock> = stmt
        .query_map([], |row| {
            Ok(WormBlock {
                idx: row.get::<_, i64>(0)? as u64,
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?).unwrap_or_default().with_timezone(&Utc),
                prev_hash: row.get(2)?,
                block_type: row.get(3)?,
                data: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                hash: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let count = blocks.len() as u64;
    for i in 1..blocks.len() {
        let block = &blocks[i];
        let prev = &blocks[i - 1];
        if block.prev_hash != prev.hash { return Ok((false, count)); }
        let computed = compute_hash(block.idx, &block.prev_hash, &block.block_type, &block.data);
        if computed != block.hash { return Ok((false, count)); }
    }
    Ok((true, count))
}

pub fn get_last_n(conn: &Connection, n: usize) -> Result<Vec<WormBlock>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT idx, timestamp, prev_hash, block_type, data, hash FROM worm_chain ORDER BY idx DESC LIMIT ?1")?;
    let blocks: Vec<WormBlock> = stmt
        .query_map([n as i64], |row| {
            Ok(WormBlock {
                idx: row.get::<_, i64>(0)? as u64,
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?).unwrap_or_default().with_timezone(&Utc),
                prev_hash: row.get(2)?,
                block_type: row.get(3)?,
                data: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                hash: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(blocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_worm_db(&conn).unwrap();
        seed_genesis(&conn).unwrap();
        conn
    }

    #[test]
    fn test_genesis_seeded() {
        let conn = setup();
        let (valid, count) = verify(&conn).unwrap();
        assert!(valid);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_append_and_verify() {
        let conn = setup();
        append(&conn, "price_fetch", serde_json::json!({"hash": "abc123"})).unwrap();
        append(&conn, "alert_fired", serde_json::json!({"symbol": "XRP", "direction": "up"})).unwrap();
        let (valid, count) = verify(&conn).unwrap();
        assert!(valid);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_get_last_n() {
        let conn = setup();
        append(&conn, "price_fetch", serde_json::json!({})).unwrap();
        append(&conn, "alert_fired", serde_json::json!({})).unwrap();
        let last2 = get_last_n(&conn, 2).unwrap();
        assert_eq!(last2.len(), 2);
        assert_eq!(last2[0].block_type, "alert_fired");
    }

    #[test]
    fn test_double_genesis_noop() {
        let conn = setup();
        seed_genesis(&conn).unwrap();
        let (valid, count) = verify(&conn).unwrap();
        assert!(valid);
        assert_eq!(count, 1);
    }
}
