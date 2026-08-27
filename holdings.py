"""Manage portfolio holdings — import from positions.txt into SQLite."""

import argparse
import csv
from pathlib import Path
from db import get_conn, init_db


POSITIONS_FILE = Path(__file__).parent / "positions.txt"


def sync_from_file():
    """Read positions.txt and upsert into holdings table."""
    init_db()
    if not POSITIONS_FILE.exists():
        print(f"No {POSITIONS_FILE} found. Create it first.")
        print("Format: SYMBOL,asset_class,quantity,avg_cost_per_unit")
        print("Example: XRP,crypto,434.841,1.05")
        return

    conn = get_conn()
    count = 0
    with open(POSITIONS_FILE, "r") as f:
        reader = csv.reader(f)
        for row in reader:
            if not row or row[0].startswith("#"):
                continue
            symbol = row[0].strip().upper()
            asset_class = row[1].strip().lower() if len(row) > 1 else "equity"
            qty = float(row[2].strip()) if len(row) > 2 else 0
            avg_cost = float(row[3].strip()) if len(row) > 3 and row[3].strip() else None

            conn.execute("""
                INSERT INTO holdings (symbol, asset_class, qty, avg_cost, source, updated_at)
                VALUES (?, ?, ?, ?, 'manual', datetime('now'))
                ON CONFLICT(symbol) DO UPDATE SET
                    asset_class = excluded.asset_class,
                    qty = excluded.qty,
                    avg_cost = excluded.avg_cost,
                    source = 'manual',
                    updated_at = datetime('now')
            """, (symbol, asset_class, qty, avg_cost))
            count += 1
            print(f"  {symbol}: {qty} {asset_class} @ ${avg_cost or '?'}")

    conn.commit()
    conn.close()
    print(f"\nSynced {count} position(s) into database.")


def list_holdings():
    """Print current holdings from the database."""
    init_db()
    conn = get_conn()
    rows = conn.execute("SELECT * FROM holdings ORDER BY symbol").fetchall()
    conn.close()

    if not rows:
        print("No holdings in database. Run: python holdings.py --sync")
        return

    print(f"{'Symbol':<10} {'Type':<8} {'Qty':>12} {'Avg Cost':>10} {'Source':<8} {'Updated'}")
    print("-" * 70)
    for r in rows:
        cost = f"${r['avg_cost']:.4f}" if r['avg_cost'] else "—"
        print(f"{r['symbol']:<10} {r['asset_class']:<8} {r['qty']:>12.4f} {cost:>10} {r['source']:<8} {r['updated_at']}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Manage portfolio holdings")
    parser.add_argument("--sync", action="store_true", help="Import positions.txt into database")
    parser.add_argument("--list", action="store_true", help="Show current holdings")
    args = parser.parse_args()

    if args.sync:
        sync_from_file()
    elif args.list:
        list_holdings()
    else:
        parser.print_help()
