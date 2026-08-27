"""Fetch live prices for all holdings using Finnhub (stocks) and CoinGecko (crypto)."""

import argparse
import requests
from db import get_conn, init_db
from config import FINNHUB_KEY

# CoinGecko symbol → id mapping for common crypto
COINGECKO_IDS = {
    "BTC": "bitcoin",
    "ETH": "ethereum",
    "XRP": "ripple",
    "DOGE": "dogecoin",
    "SOL": "solana",
    "ADA": "cardano",
    "DOT": "polkadot",
    "MATIC": "matic-network",
    "LINK": "chainlink",
    "AVAX": "avalanche-2",
    "SHIB": "shiba-inu",
    "LTC": "litecoin",
    "BNB": "binancecoin",
}


def get_crypto_prices(symbols):
    """Fetch crypto prices from CoinGecko free API."""
    ids = []
    symbol_to_id = {}
    for s in symbols:
        s_upper = s.upper().replace("-USD", "")
        cg_id = COINGECKO_IDS.get(s_upper)
        if cg_id:
            ids.append(cg_id)
            symbol_to_id[cg_id] = s_upper

    if not ids:
        return {}

    url = "https://api.coingecko.com/api/v3/simple/price"
    params = {"ids": ",".join(ids), "vs_currencies": "usd", "include_24hr_change": "true"}
    resp = requests.get(url, params=params, timeout=10)
    resp.raise_for_status()
    data = resp.json()

    prices = {}
    for cg_id, info in data.items():
        symbol = symbol_to_id.get(cg_id, cg_id)
        prices[symbol] = {
            "price": info.get("usd", 0),
            "change_pct": round(info.get("usd_24h_change", 0), 2),
        }
    return prices


def get_stock_price(symbol):
    """Fetch a single stock price from Finnhub."""
    if not FINNHUB_KEY:
        return None
    url = f"https://finnhub.io/api/v1/quote?symbol={symbol}&token={FINNHUB_KEY}"
    resp = requests.get(url, timeout=10)
    resp.raise_for_status()
    data = resp.json()
    if data.get("c", 0) == 0:
        return None
    return {
        "price": data["c"],
        "change_pct": round(data.get("dp", 0), 2),
    }


def fetch_all_prices():
    """Fetch prices for all holdings in the database."""
    init_db()
    conn = get_conn()
    rows = conn.execute("SELECT symbol, asset_class, qty, avg_cost FROM holdings").fetchall()
    conn.close()

    if not rows:
        print("No holdings found. Run: python holdings.py --sync")
        return {}

    crypto_symbols = [r["symbol"] for r in rows if r["asset_class"] == "crypto"]
    stock_symbols = [r["symbol"] for r in rows if r["asset_class"] == "equity"]

    all_prices = {}

    # Batch fetch crypto
    if crypto_symbols:
        all_prices.update(get_crypto_prices(crypto_symbols))

    # Individual fetch stocks
    for sym in stock_symbols:
        result = get_stock_price(sym)
        if result:
            all_prices[sym] = result

    return all_prices


def print_prices():
    """Print a formatted price table for all holdings."""
    init_db()
    conn = get_conn()
    rows = conn.execute("SELECT symbol, asset_class, qty, avg_cost FROM holdings").fetchall()
    conn.close()

    prices = fetch_all_prices()

    print(f"\n{'Symbol':<8} {'Price':>10} {'24h':>8} {'Qty':>12} {'Value':>12} {'P/L':>12}")
    print("-" * 68)

    total_value = 0
    total_cost = 0

    for r in rows:
        sym = r["symbol"]
        qty = r["qty"]
        avg_cost = r["avg_cost"]
        p = prices.get(sym)

        if p:
            price = p["price"]
            change = p["change_pct"]
            value = price * qty
            total_value += value

            if avg_cost:
                cost_basis = avg_cost * qty
                total_cost += cost_basis
                pl = value - cost_basis
                pl_str = f"{'+'if pl>=0 else ''}{pl:.2f}"
            else:
                pl_str = "—"

            change_str = f"{'+'if change>=0 else ''}{change:.1f}%"
            print(f"{sym:<8} ${price:>9.4f} {change_str:>8} {qty:>12.4f} ${value:>10.2f} {pl_str:>12}")
        else:
            print(f"{sym:<8} {'no data':>10}")

    print("-" * 68)
    if total_cost > 0:
        total_pl = total_value - total_cost
        total_pct = (total_pl / total_cost) * 100
        print(f"{'TOTAL':<8} {'':>10} {'':>8} {'':>12} ${total_value:>10.2f} {'+'if total_pl>=0 else ''}{total_pl:.2f} ({total_pct:+.1f}%)")
    else:
        print(f"{'TOTAL':<8} {'':>10} {'':>8} {'':>12} ${total_value:>10.2f}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Fetch live prices for holdings")
    parser.add_argument("--json", action="store_true", help="Output as JSON")
    args = parser.parse_args()

    if args.json:
        import json
        print(json.dumps(fetch_all_prices(), indent=2))
    else:
        print_prices()
