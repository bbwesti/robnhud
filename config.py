"""Shared configuration — loads .env once, used by all modules."""

import os
from pathlib import Path
from dotenv import load_dotenv

load_dotenv(Path(__file__).parent / ".env")

FINNHUB_KEY = os.getenv("FINNHUB_KEY", "")
NTFY_TOPIC = os.getenv("NTFY_TOPIC", "")
GMAIL_ADDRESS = os.getenv("GMAIL_ADDRESS", "")
GMAIL_APP_PASSWORD = os.getenv("GMAIL_APP_PASSWORD", "")
DIGEST_TO = os.getenv("DIGEST_TO", GMAIL_ADDRESS)
SEC_USER_AGENT = os.getenv("SEC_USER_AGENT", "")
TIMEZONE = os.getenv("TIMEZONE", "America/Los_Angeles")
POLL_SECONDS = int(os.getenv("POLL_SECONDS", "60"))
ALERT_COOLDOWN_MINUTES = int(os.getenv("ALERT_COOLDOWN_MINUTES", "45"))
HYSTERESIS_PCT = float(os.getenv("HYSTERESIS_PCT", "0.75"))

DB_PATH = Path(__file__).parent / "robnhud.db"
