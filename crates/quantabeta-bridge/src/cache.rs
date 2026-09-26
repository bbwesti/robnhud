use rusqlite::Connection;
use crate::analysis::QuantaBetaReport;

pub fn init_cache_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS quantabeta_cache (
            id INTEGER PRIMARY KEY,
            report_json TEXT NOT NULL,
            data_hash TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );"
    )
}

pub fn store_report(conn: &Connection, report: &QuantaBetaReport, data_hash: &str) -> rusqlite::Result<()> {
    let json = serde_json::to_string(report).unwrap_or_default();
    conn.execute(
        "INSERT INTO quantabeta_cache (report_json, data_hash) VALUES (?1, ?2)",
        rusqlite::params![json, data_hash],
    )?;
    conn.execute(
        "DELETE FROM quantabeta_cache WHERE id NOT IN (SELECT id FROM quantabeta_cache ORDER BY id DESC LIMIT 100)",
        [],
    )?;
    Ok(())
}

pub fn get_cached_report(conn: &Connection, current_data_hash: &str) -> Option<QuantaBetaReport> {
    conn.query_row(
        "SELECT report_json FROM quantabeta_cache WHERE data_hash = ?1 ORDER BY id DESC LIMIT 1",
        rusqlite::params![current_data_hash],
        |row| row.get::<_, String>(0),
    ).ok().and_then(|json| serde_json::from_str(&json).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{AssetAnalysis, QuantaBetaReport};

    fn make_test_report() -> QuantaBetaReport {
        QuantaBetaReport {
            assets: vec![AssetAnalysis {
                symbol: "TEST".to_string(),
                partition_volatility: vec![0.5, 0.6],
                entropy_point: 1.5,
                entropy_interval: (1.4, 1.6),
                complexity_bound: "42".to_string(),
                ramanujan_congruence_holds: true,
                data_points: 20,
                audit_hash: "abc123".to_string(),
            }],
            cross_correlations: vec![],
            timestamp: "2026-09-26T00:00:00Z".to_string(),
            report_hash: "hash123".to_string(),
        }
    }

    #[test]
    fn test_cache_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        init_cache_table(&conn).unwrap();
        let report = make_test_report();
        store_report(&conn, &report, "datahash1").unwrap();
        let cached = get_cached_report(&conn, "datahash1").unwrap();
        assert_eq!(cached.assets[0].symbol, "TEST");
    }

    #[test]
    fn test_cache_miss_on_different_hash() {
        let conn = Connection::open_in_memory().unwrap();
        init_cache_table(&conn).unwrap();
        store_report(&conn, &make_test_report(), "datahash1").unwrap();
        assert!(get_cached_report(&conn, "different").is_none());
    }
}
