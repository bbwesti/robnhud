//! CRYPTEK VIGIL — WORM Chain (Write Once Read Many)
//!
//! Append-only SHA-256 hash-chained audit trail backed by SQLite.
//! Audit_Spec: 4b565498-9afc-4782-af4a-c6b11a5d0058

pub mod keys;

use chrono::{DateTime, Utc};
use ed25519_dalek::SigningKey;
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
    pub signature: String,
    pub pubkey: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub chain_valid: bool,
    pub all_signatures_valid: bool,
    pub block_count: u64,
    pub invalid_blocks: Vec<u64>,
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

pub fn migrate_signed_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
    let has_sig: bool = conn.prepare("SELECT signature FROM worm_chain LIMIT 0").is_ok();
    if !has_sig {
        conn.execute_batch(
            "ALTER TABLE worm_chain ADD COLUMN signature TEXT NOT NULL DEFAULT '';
             ALTER TABLE worm_chain ADD COLUMN pubkey TEXT NOT NULL DEFAULT '';
             ALTER TABLE worm_chain ADD COLUMN hash_algo TEXT NOT NULL DEFAULT 'sha256';",
        )?;
    }
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

fn compute_blake3_hash(idx: u64, prev_hash: &str, block_type: &str, data: &serde_json::Value) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(idx.to_string().as_bytes());
    hasher.update(prev_hash.as_bytes());
    hasher.update(block_type.as_bytes());
    hasher.update(&serde_json::to_vec(data).unwrap_or_default());
    hasher.finalize().to_hex().to_string()
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
    Ok(WormBlock { idx, timestamp: now, prev_hash: last_hash, block_type: block_type.to_string(), data, hash, signature: String::new(), pubkey: String::new() })
}

pub fn append_signed(
    conn: &Connection,
    block_type: &str,
    data: serde_json::Value,
    signing_key: &SigningKey,
) -> Result<WormBlock, rusqlite::Error> {
    migrate_signed_columns(conn)?;
    let (last_idx, last_hash): (u64, String) = conn
        .query_row("SELECT idx, hash FROM worm_chain ORDER BY idx DESC LIMIT 1", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap_or((0, String::new()));
    let idx = last_idx + 1;
    let hash = compute_blake3_hash(idx, &last_hash, block_type, &data);
    let sig = keys::sign_message(signing_key, hash.as_bytes());
    let sig_hex = keys::signature_to_hex(&sig);
    let pk_hex = keys::pubkey_to_hex(&signing_key.verifying_key());
    let now = Utc::now();
    conn.execute(
        "INSERT INTO worm_chain (idx, timestamp, prev_hash, block_type, data, hash, signature, pubkey, hash_algo)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            idx as i64, now.to_rfc3339(), last_hash, block_type,
            serde_json::to_string(&data).unwrap(), hash, sig_hex, pk_hex, "blake3"
        ],
    )?;
    Ok(WormBlock {
        idx, timestamp: now, prev_hash: last_hash,
        block_type: block_type.to_string(), data, hash,
        signature: sig_hex, pubkey: pk_hex,
    })
}

pub fn verify(conn: &Connection) -> Result<(bool, u64), rusqlite::Error> {
    migrate_signed_columns(conn)?;
    let mut stmt = conn.prepare("SELECT idx, timestamp, prev_hash, block_type, data, hash, COALESCE(signature,'') as signature, COALESCE(pubkey,'') as pubkey, COALESCE(hash_algo,'sha256') as hash_algo FROM worm_chain ORDER BY idx")?;
    let blocks: Vec<(WormBlock, String)> = stmt
        .query_map([], |row| {
            let hash_algo: String = row.get(8)?;
            Ok((WormBlock {
                idx: row.get::<_, i64>(0)? as u64,
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?).unwrap_or_default().with_timezone(&Utc),
                prev_hash: row.get(2)?,
                block_type: row.get(3)?,
                data: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                hash: row.get(5)?,
                signature: row.get(6)?,
                pubkey: row.get(7)?,
            }, hash_algo))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let count = blocks.len() as u64;
    for i in 1..blocks.len() {
        let (block, algo) = &blocks[i];
        let (prev, _) = &blocks[i - 1];
        if block.prev_hash != prev.hash { return Ok((false, count)); }
        let computed = if algo == "blake3" {
            compute_blake3_hash(block.idx, &block.prev_hash, &block.block_type, &block.data)
        } else {
            compute_hash(block.idx, &block.prev_hash, &block.block_type, &block.data)
        };
        if computed != block.hash { return Ok((false, count)); }
    }
    Ok((true, count))
}

pub fn verify_signed(conn: &Connection, expected_pk: &ed25519_dalek::VerifyingKey) -> Result<VerifyReport, rusqlite::Error> {
    migrate_signed_columns(conn)?;
    let mut stmt = conn.prepare(
        "SELECT idx, timestamp, prev_hash, block_type, data, hash, COALESCE(signature,'') as signature, COALESCE(pubkey,'') as pubkey, COALESCE(hash_algo,'sha256') as hash_algo FROM worm_chain ORDER BY idx"
    )?;
    let blocks: Vec<(WormBlock, String)> = stmt
        .query_map([], |row| {
            let hash_algo: String = row.get(8)?;
            Ok((WormBlock {
                idx: row.get::<_, i64>(0)? as u64,
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?).unwrap_or_default().with_timezone(&Utc),
                prev_hash: row.get(2)?,
                block_type: row.get(3)?,
                data: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                hash: row.get(5)?,
                signature: row.get(6)?,
                pubkey: row.get(7)?,
            }, hash_algo))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let block_count = blocks.len() as u64;
    let mut chain_valid = true;
    let mut all_sigs_valid = true;
    let mut invalid_blocks = Vec::new();
    for i in 1..blocks.len() {
        let (block, algo) = &blocks[i];
        let (prev, _) = &blocks[i - 1];
        if block.prev_hash != prev.hash {
            chain_valid = false;
            invalid_blocks.push(block.idx);
            continue;
        }
        let computed = if algo == "blake3" {
            compute_blake3_hash(block.idx, &block.prev_hash, &block.block_type, &block.data)
        } else {
            compute_hash(block.idx, &block.prev_hash, &block.block_type, &block.data)
        };
        if computed != block.hash {
            chain_valid = false;
            invalid_blocks.push(block.idx);
            continue;
        }
        if !block.signature.is_empty() {
            if let Some(sig) = keys::signature_from_hex(&block.signature) {
                if !keys::verify_signature(expected_pk, block.hash.as_bytes(), &sig) {
                    all_sigs_valid = false;
                    invalid_blocks.push(block.idx);
                }
            } else {
                all_sigs_valid = false;
                invalid_blocks.push(block.idx);
            }
        }
    }
    Ok(VerifyReport { chain_valid, all_signatures_valid: all_sigs_valid, block_count, invalid_blocks })
}

pub fn get_last_n(conn: &Connection, n: usize) -> Result<Vec<WormBlock>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT idx, timestamp, prev_hash, block_type, data, hash, COALESCE(signature,'') as signature, COALESCE(pubkey,'') as pubkey FROM worm_chain ORDER BY idx DESC LIMIT ?1")?;
    let blocks: Vec<WormBlock> = stmt
        .query_map([n as i64], |row| {
            Ok(WormBlock {
                idx: row.get::<_, i64>(0)? as u64,
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(1)?).unwrap_or_default().with_timezone(&Utc),
                prev_hash: row.get(2)?,
                block_type: row.get(3)?,
                data: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                hash: row.get(5)?,
                signature: row.get(6)?,
                pubkey: row.get(7)?,
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
        migrate_signed_columns(&conn).unwrap();
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

    #[test]
    fn test_signed_append_and_verify() {
        let conn = setup();
        let key_dir = std::env::temp_dir().join("vigil_test_signed_worm");
        let _ = std::fs::remove_dir_all(&key_dir);
        std::fs::create_dir_all(&key_dir).unwrap();
        let sk = crate::keys::load_or_generate_signing_key(&key_dir).unwrap();

        append_signed(&conn, "price_fetch", serde_json::json!({"hash": "abc"}), &sk).unwrap();
        append_signed(&conn, "alert_fired", serde_json::json!({"sym": "XRP"}), &sk).unwrap();

        let report = verify_signed(&conn, &sk.verifying_key()).unwrap();
        assert!(report.chain_valid);
        assert!(report.all_signatures_valid);
        assert_eq!(report.block_count, 3);

        std::fs::remove_dir_all(&key_dir).ok();
    }

    #[test]
    fn test_blake3_hash_used() {
        let conn = setup();
        let key_dir = std::env::temp_dir().join("vigil_test_blake3");
        let _ = std::fs::remove_dir_all(&key_dir);
        std::fs::create_dir_all(&key_dir).unwrap();
        let sk = crate::keys::load_or_generate_signing_key(&key_dir).unwrap();

        let block = append_signed(&conn, "test", serde_json::json!({}), &sk).unwrap();
        assert_eq!(block.hash.len(), 64);
        assert!(!block.signature.is_empty());
        assert!(!block.pubkey.is_empty());

        std::fs::remove_dir_all(&key_dir).ok();
    }
}
