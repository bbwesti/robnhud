"""Filtered news feed — fetches news for your holdings from multiple sources."""

import argparse
import hashlib
import time
import requests
from db import get_conn, init_db
from config import FINNHUB_KEY

# CoinGecko symbol → id mapping (same as prices.py)
COINGECKO_IDS = {
    "BTC": "bitcoin", "ETH": "ethereum", "XRP": "ripple", "DOGE": "dogecoin",
    "SOL": "solana", "ADA": "cardano", "DOT": "polkadot", "LINK": "chainlink",
    "AVAX": "avalanche-2", "SHIB": "shiba-inu", "LTC": "litecoin", "BNB": "binancecoin",
    "MATIC": "matic-network",
}

# Common full names for headline matching
CRYPTO_NAMES = {
    "XRP": ["xrp", "ripple"],
    "DOGE": ["doge", "dogecoin"],
    "BTC": ["bitcoin", "btc"],
    "ETH": ["ethereum", "eth"],
    "SOL": ["solana", "sol"],
    "ADA": ["cardano", "ada"],
    "LINK": ["chainlink"],
    "SHIB": ["shiba", "shib"],
    "LTC": ["litecoin"],
    "BNB": ["bnb", "binance coin"],
}


def get_finnhub_news(symbol):
    """Fetch recent news for a stock symbol from Finnhub."""
    if not FINNHUB_KEY:
        return []
    from datetime import datetime, timedelta
    today = datetime.now().strftime("%Y-%m-%d")
    week_ago = (datetime.now() - timedelta(days=7)).strftime("%Y-%m-%d")
    params = {"symbol": symbol, "from": week_ago, "to": today, "token": FINNHUB_KEY}
    try:
        resp = requests.get("https://finnhub.io/api/v1/company-news", params=params, timeout=10)
        resp.raise_for_status()
        return resp.json()
    except requests.exceptions.HTTPError:
        print(f"Finnhub company news error for {symbol}")
        return []


def get_finnhub_general_news():
    """Fetch general market news from Finnhub."""
    if not FINNHUB_KEY:
        return []
    try:
        resp = requests.get(
            f"https://finnhub.io/api/v1/news?category=general&token={FINNHUB_KEY}", timeout=10
        )
        resp.raise_for_status()
        return resp.json()[:30]
    except requests.exceptions.HTTPError:
        print("Finnhub general news error")
        return []


def get_finnhub_crypto_news():
    """Fetch crypto-specific news from Finnhub."""
    if not FINNHUB_KEY:
        return []
    try:
        resp = requests.get(
            f"https://finnhub.io/api/v1/news?category=crypto&token={FINNHUB_KEY}", timeout=10
        )
        resp.raise_for_status()
        return resp.json()[:30]
    except requests.exceptions.HTTPError:
        print("Finnhub crypto news error")
        return []


def get_coingecko_news():
    """Fetch trending crypto news from CoinGecko (no key needed)."""
    try:
        resp = requests.get(
            "https://api.coingecko.com/api/v3/news",
            params={"per_page": 25},
            timeout=10,
        )
        resp.raise_for_status()
        data = resp.json()
        # CoinGecko news format is different — normalize it
        articles = []
        items = data if isinstance(data, list) else data.get("data", [])
        for item in items:
            articles.append({
                "headline": item.get("title", ""),
                "url": item.get("url", ""),
                "source": item.get("author", item.get("news_site", "CoinGecko")),
                "datetime": 0,  # CoinGecko uses different timestamp format
                "summary": item.get("description", "")[:200],
                "related": "",
            })
        return articles
    except Exception:
        return []


def get_cryptopanic_news(symbols):
    """Fetch crypto news from CryptoPanic public RSS-like feed (no key needed)."""
    currencies = ",".join(s.upper() for s in symbols)
    try:
        resp = requests.get(
            "https://cryptopanic.com/api/free/v1/posts/",
            params={"auth_token": "free", "currencies": currencies, "public": "true"},
            timeout=10,
        )
        if resp.status_code == 200:
            data = resp.json()
            articles = []
            for item in data.get("results", [])[:20]:
                articles.append({
                    "headline": item.get("title", ""),
                    "url": item.get("url", ""),
                    "source": item.get("source", {}).get("title", "CryptoPanic"),
                    "datetime": 0,
                    "summary": "",
                    "related": ",".join(c.get("code", "") for c in item.get("currencies", [])),
                })
            return articles
    except Exception:
        pass
    return []


def headline_matches_holdings(headline, holdings):
    """Check if a headline mentions any of our holdings by symbol or name."""
    headline_lower = headline.lower()
    for h in holdings:
        sym = h["symbol"].upper()
        # Check symbol
        if sym.lower() in headline_lower:
            return True
        # Check common names
        names = CRYPTO_NAMES.get(sym, [])
        for name in names:
            if name in headline_lower:
                return True
    return False


def fetch_news_for_holdings():
    """Fetch and deduplicate news for all holdings from multiple sources."""
    init_db()
    conn = get_conn()
    holdings = conn.execute("SELECT symbol, asset_class FROM holdings").fetchall()

    all_news = []
    seen_hashes = set()

    # Load already-seen hashes
    for row in conn.execute("SELECT url_hash FROM news_seen").fetchall():
        seen_hashes.add(row["url_hash"])

    crypto_holdings = [h for h in holdings if h["asset_class"] == "crypto"]
    stock_holdings = [h for h in holdings if h["asset_class"] == "equity"]
    crypto_symbols = [h["symbol"] for h in crypto_holdings]

    # Collect articles from all sources
    raw_articles = []

    # Finnhub crypto news
    raw_articles.extend(get_finnhub_crypto_news())
    time.sleep(0.3)  # Rate limit courtesy

    # Finnhub general market news
    raw_articles.extend(get_finnhub_general_news())
    time.sleep(0.3)

    # CoinGecko news
    raw_articles.extend(get_coingecko_news())

    # Stock-specific news from Finnhub
    for h in stock_holdings:
        raw_articles.extend(get_finnhub_news(h["symbol"]))
        time.sleep(0.3)

    # Deduplicate and filter
    for article in raw_articles:
        url = article.get("url", "")
        headline = article.get("headline", "")
        if not url or not headline:
            continue

        url_hash = hashlib.md5(url.encode()).hexdigest()
        if url_hash in seen_hashes:
            continue
        seen_hashes.add(url_hash)

        # Include if it mentions any holding, or if it's from a stock-specific query
        relevant = headline_matches_holdings(headline, holdings)
        # Also include general crypto news if we hold any crypto
        if not relevant and crypto_holdings:
            crypto_keywords = ["crypto", "bitcoin", "blockchain", "defi", "token", "coin"]
            relevant = any(kw in headline.lower() for kw in crypto_keywords)

        if relevant:
            all_news.append({
                "headline": headline,
                "url": url,
                "source": article.get("source", "Unknown"),
                "datetime": article.get("datetime", 0),
                "related": article.get("related", ""),
                "summary": article.get("summary", ""),
                "url_hash": url_hash,
            })

            conn.execute(
                "INSERT OR IGNORE INTO news_seen (url_hash, headline) VALUES (?, ?)",
                (url_hash, headline),
            )

    conn.commit()
    conn.close()

    all_news.sort(key=lambda x: x["datetime"], reverse=True)
    return all_news


def print_news(limit=20):
    """Print a formatted news feed."""
    import sys, io
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')

    news = fetch_news_for_holdings()

    if not news:
        print("No new articles found for your holdings.")
        return

    print(f"\nNews Feed -- {len(news)} articles\n")
    for article in news[:limit]:
        from datetime import datetime
        ts = article["datetime"]
        if ts:
            dt = datetime.fromtimestamp(ts).strftime("%b %d %I:%M%p")
        else:
            dt = "Recent"
        print(f"  [{dt}] {article['source']}")
        print(f"  {article['headline']}")
        if article['related']:
            print(f"  Related: {article['related']}")
        print(f"  {article['url']}")
        print()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="News feed for your holdings")
    parser.add_argument("--limit", type=int, default=20, help="Max articles to show")
    parser.add_argument("--json", action="store_true", help="Output as JSON")
    args = parser.parse_args()

    if args.json:
        import json
        print(json.dumps(fetch_news_for_holdings()[:args.limit], indent=2))
    else:
        print_news(args.limit)
