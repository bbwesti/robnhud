use crate::{AssetClass, Holding, Result};
use rusqlite::Connection;
use std::path::Path;

pub fn init_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS holdings (
            symbol     TEXT PRIMARY KEY,
            asset_class TEXT NOT NULL,
            qty        REAL NOT NULL,
            avg_cost   REAL NOT NULL DEFAULT 0.0
        );
        CREATE TABLE IF NOT EXISTS price_history (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            symbol    TEXT NOT NULL,
            price     REAL NOT NULL,
            change_pct REAL NOT NULL DEFAULT 0.0,
            timestamp TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS news (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            title     TEXT NOT NULL,
            url       TEXT,
            source    TEXT,
            published TEXT,
            relevance_tag TEXT
        );
        CREATE TABLE IF NOT EXISTS alert_state (
            symbol    TEXT PRIMARY KEY,
            baseline  REAL NOT NULL,
            last_fired TEXT,
            last_direction TEXT
        );",
    )?;
    Ok(conn)
}

pub fn get_holdings(conn: &Connection) -> Result<Vec<Holding>> {
    let mut stmt = conn.prepare("SELECT symbol, asset_class, qty, avg_cost FROM holdings")?;
    let holdings = stmt
        .query_map([], |row| {
            Ok(Holding {
                symbol: row.get(0)?,
                asset_class: AssetClass::from_str(&row.get::<_, String>(1)?),
                qty: row.get(2)?,
                avg_cost: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(holdings)
}

pub fn import_positions(conn: &Connection, positions_path: &Path) -> Result<usize> {
    let content = std::fs::read_to_string(positions_path)?;
    let mut count = 0;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 4 {
            continue;
        }
        let symbol = parts[0].trim();
        let asset_class = parts[1].trim();
        let qty: f64 = parts[2].trim().parse().unwrap_or(0.0);
        let avg_cost: f64 = parts[3].trim().parse().unwrap_or(0.0);

        conn.execute(
            "INSERT OR REPLACE INTO holdings (symbol, asset_class, qty, avg_cost) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![symbol, asset_class, qty, avg_cost],
        )?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_init_and_import() {
        let conn = init_db(Path::new(":memory:")).unwrap();

        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, "XRP,crypto,434.841,1.05").unwrap();
        writeln!(tmp, "DOGE,crypto,1.72,0.15").unwrap();

        let count = import_positions(&conn, tmp.path()).unwrap();
        assert_eq!(count, 2);

        let holdings = get_holdings(&conn).unwrap();
        assert_eq!(holdings.len(), 2);
        assert_eq!(holdings[0].symbol, "XRP");
        assert_eq!(holdings[0].asset_class, AssetClass::Crypto);
        assert!((holdings[0].qty - 434.841).abs() < 0.001);
    }
}
