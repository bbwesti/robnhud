"""Main scheduler — runs alerts, dashboard, tunnel, and sends the digest at 18:00."""

import subprocess
import sys
import threading
import time
import requests
from datetime import datetime
from pathlib import Path
import schedule
from config import POLL_SECONDS, NTFY_TOPIC
from alerts import check_alerts, recalibrate_pct_alerts
from digest import send_digest

PROJECT_DIR = Path(__file__).parent
CLOUDFLARED = PROJECT_DIR / "cloudflared.exe"
TUNNEL_LOG = PROJECT_DIR / "tunnel.log"
TUNNEL_URL_FILE = PROJECT_DIR / "tunnel_url.txt"


def digest_job():
    print(f"\n[{datetime.now().strftime('%H:%M')}] Sending daily digest...")
    send_digest()
    print("Digest sent.\n")


def recalibrate_job():
    print(f"[{datetime.now().strftime('%H:%M')}] Recalibrating percentage alerts...")
    recalibrate_pct_alerts()


def alert_job():
    try:
        check_alerts()
    except Exception as e:
        print(f"Alert check error: {e}")


def run_dashboard():
    """Run the web dashboard in a background thread."""
    from dashboard import app
    app.run(host="0.0.0.0", port=5555, debug=False, use_reloader=False)


def _extract_tunnel_url():
    """Parse tunnel.log for the trycloudflare URL."""
    try:
        text = TUNNEL_LOG.read_text()
        for line in text.splitlines():
            if "trycloudflare.com" in line and "https://" in line:
                start = line.find("https://")
                end = line.find(".trycloudflare.com") + len(".trycloudflare.com")
                if start >= 0 and end > start:
                    return line[start:end]
    except Exception:
        pass
    return None


def _notify_tunnel_url(url):
    """Send push notification with the new tunnel URL."""
    if not NTFY_TOPIC:
        return
    try:
        requests.post(
            f"https://ntfy.sh/{NTFY_TOPIC}",
            data=url,
            headers={
                "Title": "robnhud — new phone URL",
                "Tags": "link",
                "Click": url,
            },
            timeout=10,
        )
    except Exception as e:
        print(f"  Tunnel:  ntfy notify failed — {e}")


def run_tunnel():
    """Start Cloudflare quick tunnel with auto-restart and URL notifications."""
    if not CLOUDFLARED.exists():
        print("  Tunnel:  cloudflared.exe not found — skipping remote access")
        return

    last_url = None
    while True:
        print(f"  [{datetime.now().strftime('%H:%M')}] Starting Cloudflare tunnel...")
        with open(TUNNEL_LOG, "w") as log:
            proc = subprocess.Popen(
                [str(CLOUDFLARED), "tunnel", "--url", "http://localhost:5555"],
                stdout=log, stderr=subprocess.STDOUT,
            )

        # Wait for tunnel URL to appear
        url = None
        for _ in range(30):  # try for up to 30 seconds
            time.sleep(1)
            url = _extract_tunnel_url()
            if url:
                break

        if url:
            print(f"  Phone URL:  {url}")
            TUNNEL_URL_FILE.write_text(url)
            if url != last_url:
                _notify_tunnel_url(url)
                last_url = url
        else:
            print("  Tunnel:  started but URL not found — check tunnel.log")

        # Wait for process to exit (means tunnel died)
        proc.wait()
        print(f"  [{datetime.now().strftime('%H:%M')}] Tunnel exited — restarting in 10s...")
        time.sleep(10)


if __name__ == "__main__":
    import socket
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("8.8.8.8", 80))
        local_ip = s.getsockname()[0]
        s.close()
    except Exception:
        local_ip = "localhost"

    print("=" * 60)
    print("  robnhud — your stock market room")
    print("=" * 60)
    print(f"  Alerts:     every {POLL_SECONDS}s")
    print(f"  Digest:     18:00 daily")
    print(f"  Recalib:    09:00 daily")
    print(f"  Dashboard:  http://{local_ip}:5555  (local)")

    # Start dashboard
    dash_thread = threading.Thread(target=run_dashboard, daemon=True)
    dash_thread.start()
    time.sleep(2)

    # Start tunnel for remote access (auto-restarts if it dies)
    tunnel_thread = threading.Thread(target=run_tunnel, daemon=True)
    tunnel_thread.start()
    time.sleep(15)  # wait for first tunnel URL

    print("=" * 60)
    print("Press Ctrl+C to stop.\n")

    # Schedule daily jobs
    schedule.every().day.at("18:00").do(digest_job)
    schedule.every().day.at("09:00").do(recalibrate_job)

    # Main loop
    while True:
        schedule.run_pending()
        alert_job()
        time.sleep(POLL_SECONDS)
