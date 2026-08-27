"""SQLite database setup and helpers."""

import sqlite3
from config import DB_PATH


def get_conn():
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA journal_mode=WAL")
    return conn


def init_db():
    conn = get_conn()
    conn.executescript("""
        CREATE TABLE IF NOT EXISTS holdings (
            symbol      TEXT NOT NULL,
            asset_class TEXT NOT NULL DEFAULT 'crypto',
            qty         REAL NOT NULL DEFAULT 0,
            avg_cost    REAL,
            source      TEXT DEFAULT 'manual',
            updated_at  TEXT DEFAULT (datetime('now')),
            PRIMARY KEY (symbol)
        );

        CREATE TABLE IF NOT EXISTS alert_rules (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            symbol          TEXT NOT NULL,
            direction       TEXT NOT NULL CHECK(direction IN ('above', 'below')),
            threshold       REAL,
            pct_threshold   REAL,
            reference_price REAL,
            cooldown_s      INTEGER NOT NULL DEFAULT 2700,
            last_fired      TEXT,
            armed           INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS alert_events (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_id     INTEGER NOT NULL,
            symbol      TEXT NOT NULL,
            price       REAL NOT NULL,
            fired_at    TEXT DEFAULT (datetime('now')),
            FOREIGN KEY (rule_id) REFERENCES alert_rules(id)
        );

        CREATE TABLE IF NOT EXISTS news_seen (
            url_hash    TEXT PRIMARY KEY,
            headline    TEXT,
            seen_at     TEXT DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS news_analysis (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            url_hash    TEXT NOT NULL,
            tickers     TEXT,
            sentiment   TEXT,
            summary     TEXT,
            analyzed_at TEXT DEFAULT (datetime('now'))
        );
    """)
    conn.commit()
    conn.close()


if __name__ == "__main__":
    init_db()
    print("Database initialized at", DB_PATH)
