"""Mobile-friendly web dashboard — access your portfolio from any device."""

import json
from datetime import datetime
from flask import Flask, render_template_string
from db import get_conn, init_db
from prices import fetch_all_prices
from news import fetch_news_for_holdings

app = Flask(__name__)

TEMPLATE = """
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>robnhud</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
            background: #0d1117; color: #e6edf3;
            padding: 16px; max-width: 600px; margin: 0 auto;
        }
        h1 { font-size: 1.4em; margin-bottom: 4px; }
        .subtitle { color: #8b949e; font-size: 0.85em; margin-bottom: 16px; }
        .card {
            background: #161b22; border: 1px solid #30363d; border-radius: 10px;
            padding: 16px; margin-bottom: 12px;
        }
        .total-value { font-size: 2em; font-weight: 700; }
        .pl-positive { color: #3fb950; }
        .pl-negative { color: #f85149; }
        .change-neutral { color: #8b949e; }
        .section-title {
            font-size: 0.8em; text-transform: uppercase; letter-spacing: 1px;
            color: #8b949e; margin-bottom: 10px; font-weight: 600;
        }
        .holding {
            display: flex; justify-content: space-between; align-items: center;
            padding: 10px 0; border-bottom: 1px solid #21262d;
        }
        .holding:last-child { border-bottom: none; }
        .holding-symbol { font-weight: 700; font-size: 1.1em; }
        .holding-qty { color: #8b949e; font-size: 0.85em; }
        .holding-value { text-align: right; }
        .holding-price { font-size: 0.85em; color: #8b949e; }
        .alert-item {
            display: flex; justify-content: space-between;
            padding: 8px 0; border-bottom: 1px solid #21262d; font-size: 0.9em;
        }
        .alert-item:last-child { border-bottom: none; }
        .armed { color: #3fb950; }
        .disarmed { color: #f85149; }
        .news-item { padding: 10px 0; border-bottom: 1px solid #21262d; }
        .news-item:last-child { border-bottom: none; }
        .news-headline { font-size: 0.9em; margin-bottom: 4px; }
        .news-meta { font-size: 0.75em; color: #8b949e; }
        .news-headline a { color: #58a6ff; text-decoration: none; }
        .news-headline a:hover { text-decoration: underline; }
        .refresh-btn {
            display: block; width: 100%; padding: 12px; margin-top: 8px;
            background: #21262d; color: #e6edf3; border: 1px solid #30363d;
            border-radius: 8px; font-size: 1em; cursor: pointer; text-align: center;
            text-decoration: none;
        }
        .refresh-btn:hover { background: #30363d; }
        .badge {
            display: inline-block; padding: 2px 8px; border-radius: 12px;
            font-size: 0.75em; font-weight: 600;
        }
        .badge-up { background: #0f2d1a; color: #3fb950; }
        .badge-down { background: #2d0f0f; color: #f85149; }
    </style>
</head>
<body>
    <h1>robnhud</h1>
    <div class="subtitle">Updated {{ updated }}</div>

    <div class="card">
        <div class="total-value">${{ "%.2f"|format(total_value) }}</div>
        {% if total_pl >= 0 %}
        <div class="pl-positive">+${{ "%.2f"|format(total_pl) }} (+{{ "%.1f"|format(total_pct) }}%) all time</div>
        {% else %}
        <div class="pl-negative">${{ "%.2f"|format(total_pl) }} ({{ "%.1f"|format(total_pct) }}%) all time</div>
        {% endif %}
    </div>

    <div class="card">
        <div class="section-title">Holdings</div>
        {% for h in holdings %}
        <div class="holding">
            <div>
                <div class="holding-symbol">{{ h.symbol }}</div>
                <div class="holding-qty">{{ "%.4f"|format(h.qty) }} {{ h.asset_class }}</div>
            </div>
            <div class="holding-value">
                <div>${{ "%.2f"|format(h.value) }}</div>
                <div class="holding-price">
                    ${{ "%.4f"|format(h.price) }}
                    {% if h.change >= 0 %}
                    <span class="badge badge-up">+{{ "%.1f"|format(h.change) }}%</span>
                    {% else %}
                    <span class="badge badge-down">{{ "%.1f"|format(h.change) }}%</span>
                    {% endif %}
                </div>
                {% if h.pl is not none %}
                <div class="{{ 'pl-positive' if h.pl >= 0 else 'pl-negative' }}" style="font-size:0.8em">
                    {{ '+' if h.pl >= 0 }}${{ "%.2f"|format(h.pl) }}
                </div>
                {% endif %}
            </div>
        </div>
        {% endfor %}
    </div>

    <div class="card">
        <div class="section-title">Price Alerts</div>
        {% if alerts %}
        {% for a in alerts %}
        <div class="alert-item">
            <div>
                {{ a.symbol }} {{ a.direction }}
                ${{ "%.4f"|format(a.threshold) }}
                {% if a.pct %}({{ a.pct }}%){% endif %}
            </div>
            <div class="{{ 'armed' if a.armed else 'disarmed' }}">
                {{ 'ARMED' if a.armed else 'fired' }}
            </div>
        </div>
        {% endfor %}
        {% else %}
        <div style="color:#8b949e">No alerts configured</div>
        {% endif %}
    </div>

    <div class="card">
        <div class="section-title">News ({{ news|length }} articles)</div>
        {% for n in news[:10] %}
        <div class="news-item">
            <div class="news-headline"><a href="{{ n.url }}" target="_blank">{{ n.headline[:100] }}</a></div>
            <div class="news-meta">{{ n.source }} &middot; {{ n.time_str }}</div>
        </div>
        {% endfor %}
        {% if not news %}
        <div style="color:#8b949e">No news yet</div>
        {% endif %}
    </div>

    <a href="/" class="refresh-btn">Refresh</a>
    <div style="text-align:center; color:#484f58; font-size:0.75em; margin-top:12px;">
        robnhud &middot; auto-refreshes on tap
    </div>
</body>
</html>
"""


@app.route("/")
def index():
    init_db()
    conn = get_conn()
    rows = conn.execute("SELECT * FROM holdings ORDER BY symbol").fetchall()
    alert_rows = conn.execute("SELECT * FROM alert_rules ORDER BY symbol, direction").fetchall()
    conn.close()

    prices = fetch_all_prices()

    holdings = []
    total_value = 0
    total_cost = 0
    for r in rows:
        sym = r["symbol"]
        p = prices.get(sym, {})
        price = p.get("price", 0)
        change = p.get("change_pct", 0)
        value = price * r["qty"]
        total_value += value
        pl = None
        if r["avg_cost"]:
            cost = r["avg_cost"] * r["qty"]
            total_cost += cost
            pl = value - cost
        holdings.append({
            "symbol": sym, "asset_class": r["asset_class"], "qty": r["qty"],
            "price": price, "change": change, "value": value, "pl": pl,
        })

    total_pl = total_value - total_cost if total_cost else 0
    total_pct = (total_pl / total_cost * 100) if total_cost else 0

    alerts = []
    for a in alert_rows:
        alerts.append({
            "symbol": a["symbol"], "direction": a["direction"],
            "threshold": a["threshold"],
            "pct": f"{a['pct_threshold']:.0f}" if a["pct_threshold"] else None,
            "armed": bool(a["armed"]),
        })

    try:
        news_raw = fetch_news_for_holdings()
    except Exception:
        news_raw = []

    news = []
    for n in news_raw[:10]:
        ts = n.get("datetime", 0)
        if ts:
            time_str = datetime.fromtimestamp(ts).strftime("%I:%M %p")
        else:
            time_str = "Recent"
        news.append({
            "headline": n["headline"], "url": n["url"],
            "source": n.get("source", ""), "time_str": time_str,
        })

    return render_template_string(
        TEMPLATE,
        holdings=holdings, alerts=alerts, news=news,
        total_value=total_value, total_pl=total_pl, total_pct=total_pct,
        updated=datetime.now().strftime("%I:%M %p, %b %d"),
    )


@app.route("/api/prices")
def api_prices():
    return json.dumps(fetch_all_prices())


@app.route("/api/holdings")
def api_holdings():
    init_db()
    conn = get_conn()
    rows = conn.execute("SELECT * FROM holdings").fetchall()
    conn.close()
    return json.dumps([dict(r) for r in rows])


if __name__ == "__main__":
    print("Dashboard running at http://0.0.0.0:5555")
    print("Open this URL on your phone (same WiFi) or laptop browser.")
    print("Press Ctrl+C to stop.\n")
    app.run(host="0.0.0.0", port=5555, debug=False)
