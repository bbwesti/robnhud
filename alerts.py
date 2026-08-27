"""Price alert engine — percentage and fixed-price alerts with ntfy push notifications."""

import argparse
import time
from datetime import datetime, timedelta
import requests
from db import get_conn, init_db
from config import NTFY_TOPIC, POLL_SECONDS, ALERT_COOLDOWN_MINUTES, HYSTERESIS_PCT
from prices import fetch_all_prices


def send_ntfy(title, message, priority="high"):
    """Push a notification via ntfy.sh."""
    if not NTFY_TOPIC:
        print(f"[NTFY disabled] {title}: {message}")
        return
    try:
        requests.post(
            f"https://ntfy.sh/{NTFY_TOPIC}",
            data=message.encode("utf-8"),
            headers={
                "Title": title,
                "Priority": priority,
                "Tags": "chart_with_upwards_trend" if "up" in title.lower() or "above" in message.lower() else "chart_with_downwards_trend",
            },
            timeout=10,
        )
    except Exception as e:
        print(f"ntfy error: {e}")


def add_alert(symbol, direction, threshold):
    """Add a fixed-price alert rule."""
    init_db()
    symbol = symbol.upper()
    direction = direction.lower()
    if direction not in ("above", "below"):
        print("Direction must be 'above' or 'below'")
        return

    conn = get_conn()
    cooldown_s = ALERT_COOLDOWN_MINUTES * 60
    conn.execute(
        "INSERT INTO alert_rules (symbol, direction, threshold, cooldown_s) VALUES (?, ?, ?, ?)",
        (symbol, direction, threshold, cooldown_s),
    )
    conn.commit()
    conn.close()
    print(f"Alert added: {symbol} {direction} ${threshold:.4f} (cooldown: {ALERT_COOLDOWN_MINUTES}min)")


def add_pct_alert(symbol, pct):
    """Add percentage-based alerts (both up and down) for a symbol.

    Captures the current price as the reference point, then alerts
    when the price moves more than `pct`% in either direction.
    """
    init_db()
    symbol = symbol.upper()
    prices = fetch_all_prices()
    p = prices.get(symbol)
    if not p:
        print(f"Cannot get current price for {symbol}. Is it in your holdings?")
        return

    ref_price = p["price"]
    cooldown_s = ALERT_COOLDOWN_MINUTES * 60
    conn = get_conn()

    # Add "above" rule — fires when price rises pct% above reference
    upper = ref_price * (1 + pct / 100)
    conn.execute(
        "INSERT INTO alert_rules (symbol, direction, threshold, pct_threshold, reference_price, cooldown_s) VALUES (?, ?, ?, ?, ?, ?)",
        (symbol, "above", upper, pct, ref_price, cooldown_s),
    )

    # Add "below" rule — fires when price drops pct% below reference
    lower = ref_price * (1 - pct / 100)
    conn.execute(
        "INSERT INTO alert_rules (symbol, direction, threshold, pct_threshold, reference_price, cooldown_s) VALUES (?, ?, ?, ?, ?, ?)",
        (symbol, "below", lower, pct, ref_price, cooldown_s),
    )

    conn.commit()
    conn.close()
    print(f"Percentage alerts added for {symbol}:")
    print(f"  Reference price: ${ref_price:.4f}")
    print(f"  Alert if UP   {pct}%+: above ${upper:.4f}")
    print(f"  Alert if DOWN {pct}%+: below ${lower:.4f}")
    print(f"  Cooldown: {ALERT_COOLDOWN_MINUTES}min")


def recalibrate_pct_alerts():
    """Reset percentage alert thresholds based on current prices.

    Call this periodically (e.g. daily) so the 3% bands track
    the latest price instead of the original reference forever.
    """
    init_db()
    prices = fetch_all_prices()
    conn = get_conn()
    rules = conn.execute("SELECT * FROM alert_rules WHERE pct_threshold IS NOT NULL").fetchall()

    for rule in rules:
        sym = rule["symbol"]
        p = prices.get(sym)
        if not p:
            continue

        current_price = p["price"]
        pct = rule["pct_threshold"]

        if rule["direction"] == "above":
            new_threshold = current_price * (1 + pct / 100)
        else:
            new_threshold = current_price * (1 - pct / 100)

        conn.execute(
            "UPDATE alert_rules SET threshold = ?, reference_price = ?, armed = 1 WHERE id = ?",
            (new_threshold, current_price, rule["id"]),
        )
        print(f"  Recalibrated {sym} {rule['direction']}: ref=${current_price:.4f} -> threshold=${new_threshold:.4f}")

    conn.commit()
    conn.close()


def remove_alert(alert_id):
    """Remove an alert rule by ID."""
    init_db()
    conn = get_conn()
    conn.execute("DELETE FROM alert_rules WHERE id = ?", (alert_id,))
    conn.commit()
    conn.close()
    print(f"Alert {alert_id} removed.")


def list_alerts():
    """Print all alert rules."""
    init_db()
    conn = get_conn()
    rows = conn.execute("SELECT * FROM alert_rules ORDER BY symbol, direction").fetchall()
    conn.close()

    if not rows:
        print("No alerts configured.")
        print("  Fixed price:  python alerts.py --add SYMBOL above/below PRICE")
        print("  Percentage:   python alerts.py --pct SYMBOL 3")
        return

    print(f"\n{'ID':>4} {'Symbol':<8} {'Dir':<6} {'Threshold':>12} {'%':>5} {'Ref Price':>12} {'Armed':>6} {'Last Fired'}")
    print("-" * 75)
    for r in rows:
        armed = "YES" if r["armed"] else "no"
        last = r["last_fired"] or "never"
        pct = f"{r['pct_threshold']:.0f}%" if r["pct_threshold"] else "—"
        ref = f"${r['reference_price']:.4f}" if r["reference_price"] else "—"
        print(f"{r['id']:>4} {r['symbol']:<8} {r['direction']:<6} ${r['threshold']:>11.4f} {pct:>5} {ref:>12} {armed:>6} {last}")


def check_alerts():
    """Check all alert rules against current prices and fire if triggered."""
    init_db()
    prices = fetch_all_prices()
    if not prices:
        return

    conn = get_conn()
    rules = conn.execute("SELECT * FROM alert_rules WHERE armed = 1").fetchall()
    now = datetime.utcnow()

    for rule in rules:
        sym = rule["symbol"]
        p = prices.get(sym)
        if not p:
            continue

        current_price = p["price"]
        threshold = rule["threshold"]
        direction = rule["direction"]

        # Check if alert should fire
        triggered = False
        if direction == "above" and current_price >= threshold:
            triggered = True
        elif direction == "below" and current_price <= threshold:
            triggered = True

        if not triggered:
            continue

        # Check cooldown
        if rule["last_fired"]:
            last = datetime.fromisoformat(rule["last_fired"])
            if now - last < timedelta(seconds=rule["cooldown_s"]):
                continue

        # Build alert message
        if rule["pct_threshold"] and rule["reference_price"]:
            pct_move = ((current_price - rule["reference_price"]) / rule["reference_price"]) * 100
            arrow = "UP" if pct_move > 0 else "DOWN"
            title = f"{sym} {arrow} {abs(pct_move):.1f}%"
            body = (
                f"{sym} moved {pct_move:+.1f}% from ${rule['reference_price']:.4f}\n"
                f"Current: ${current_price:.4f}\n"
                f"Threshold was: ${threshold:.4f}"
            )
        else:
            title = f"{sym} {'above' if direction == 'above' else 'below'} ${threshold:.4f}"
            body = f"{sym} is {direction} ${threshold:.4f}\nCurrent: ${current_price:.4f}"

        print(f"ALERT: {title} — {body}")
        send_ntfy(title, body)

        # Log event
        conn.execute(
            "INSERT INTO alert_events (rule_id, symbol, price) VALUES (?, ?, ?)",
            (rule["id"], sym, current_price),
        )
        conn.execute(
            "UPDATE alert_rules SET last_fired = ? WHERE id = ?",
            (now.isoformat(), rule["id"]),
        )

        # Disarm until price comes back within hysteresis band
        conn.execute("UPDATE alert_rules SET armed = 0 WHERE id = ?", (rule["id"],))

    # Re-arm disarmed alerts when price pulls back
    disarmed = conn.execute("SELECT * FROM alert_rules WHERE armed = 0").fetchall()
    for rule in disarmed:
        sym = rule["symbol"]
        p = prices.get(sym)
        if not p:
            continue

        current_price = p["price"]
        threshold = rule["threshold"]
        hyst = threshold * (HYSTERESIS_PCT / 100)

        rearm = False
        if rule["direction"] == "above" and current_price < threshold - hyst:
            rearm = True
        elif rule["direction"] == "below" and current_price > threshold + hyst:
            rearm = True

        if rearm:
            # For percentage alerts, also update the threshold to track the new price
            if rule["pct_threshold"]:
                pct = rule["pct_threshold"]
                if rule["direction"] == "above":
                    new_threshold = current_price * (1 + pct / 100)
                else:
                    new_threshold = current_price * (1 - pct / 100)
                conn.execute(
                    "UPDATE alert_rules SET armed = 1, reference_price = ?, threshold = ? WHERE id = ?",
                    (current_price, new_threshold, rule["id"]),
                )
                print(f"Re-armed + recalibrated: {sym} {rule['direction']} -> ${new_threshold:.4f}")
            else:
                conn.execute("UPDATE alert_rules SET armed = 1 WHERE id = ?", (rule["id"],))
                print(f"Re-armed: {sym} {rule['direction']} ${threshold:.4f}")

    conn.commit()
    conn.close()


def run_loop():
    """Continuously poll prices and check alerts."""
    print(f"Alert loop started — polling every {POLL_SECONDS}s")
    print("Press Ctrl+C to stop.\n")
    while True:
        try:
            check_alerts()
        except Exception as e:
            print(f"Error: {e}")
        time.sleep(POLL_SECONDS)


def test_notification():
    """Send a test notification to verify ntfy is working."""
    print("Sending test notification...")
    send_ntfy(
        "robnhud Test",
        "If you see this, your alert pipeline is working!",
        priority="default",
    )
    print("Done. Check your phone for the notification.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Price alert engine")
    parser.add_argument("--add", nargs=3, metavar=("SYMBOL", "above/below", "PRICE"),
                        help="Add a fixed-price alert")
    parser.add_argument("--pct", nargs=2, metavar=("SYMBOL", "PERCENT"),
                        help="Add percentage alerts (up AND down) for a symbol")
    parser.add_argument("--recalibrate", action="store_true",
                        help="Reset all percentage alerts to current prices")
    parser.add_argument("--remove", type=int, metavar="ID", help="Remove an alert by ID")
    parser.add_argument("--list", action="store_true", help="List all alert rules")
    parser.add_argument("--check", action="store_true", help="Check alerts once")
    parser.add_argument("--run", action="store_true", help="Run alert loop continuously")
    parser.add_argument("--test", action="store_true", help="Send a test notification")
    args = parser.parse_args()

    if args.add:
        add_alert(args.add[0], args.add[1], float(args.add[2]))
    elif args.pct:
        add_pct_alert(args.pct[0], float(args.pct[1]))
    elif args.recalibrate:
        recalibrate_pct_alerts()
    elif args.remove:
        remove_alert(args.remove)
    elif args.list:
        list_alerts()
    elif args.check:
        check_alerts()
    elif args.run:
        run_loop()
    elif args.test:
        test_notification()
    else:
        parser.print_help()
