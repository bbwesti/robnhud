# robnhud Project — Session Summary (Aug 25, 2026)

## What Was Built
A personal stock/crypto market monitoring system on Windows at `C:\Users\BmAn\Desktop\robnhud\`

## Portfolio
- **XRP**: 434.841 coins, avg cost $1.05/coin (~$643 value, +41% profit)
- **DOGE**: 1.72 coins, avg cost $0.15/coin (~$0.15 value)
- Total portfolio: ~$643

## Files Created
| File | Purpose |
|------|---------|
| `config.py` | Loads .env settings, shared by all modules |
| `db.py` | SQLite database (holdings, alerts, news tables) |
| `holdings.py` | Import/manage positions from positions.txt |
| `prices.py` | Live prices via CoinGecko (crypto) + Finnhub (stocks) |
| `alerts.py` | Price alert engine with ntfy push + percentage-based alerts |
| `news.py` | Crypto news from Finnhub crypto endpoint + CoinGecko |
| `digest.py` | End-of-day email summary (portfolio + alerts + news) |
| `dashboard.py` | Mobile-friendly Flask web dashboard on port 5555 |
| `scheduler.py` | Runs everything: alerts every 60s, digest at 18:00, dashboard, Cloudflare tunnel |
| `positions.txt` | Holdings: XRP and DOGE with quantities and cost basis |
| `.env` | API keys (Finnhub, ntfy, Gmail) |
| `robnhud.db` | SQLite database (auto-created) |
| `cloudflared.exe` | Cloudflare tunnel binary for remote phone access |
| `start_robnhud.vbs` | Silent launcher for Windows auto-start |
| `start_robnhud.bat` | Batch launcher alternative |
| `requirements.txt` | Python deps: requests, python-dotenv, schedule, flask |

## API Keys Configured in .env
- **FINNHUB_KEY**: da6gqv9r01qna3celpvgda6gqv9r01qna3celq00 (40-char key, confirmed working)
- **NTFY_TOPIC**: bw-mkt-8f3k2q9x (push notifications confirmed working)
- **GMAIL_ADDRESS**: branwesterhoff@gmail.com
- **GMAIL_APP_PASSWORD**: configured (digest email confirmed sent and received)
- **TIMEZONE**: America/Los_Angeles

## What's Working
- ✅ Live crypto prices (CoinGecko free API, no key needed)
- ✅ Finnhub stock prices and crypto news (API key working)
- ✅ 3% price alerts for XRP and DOGE (up AND down, with hysteresis + cooldown)
- ✅ ntfy push notifications (test received on phone)
- ✅ Gmail digest email (test sent and received)
- ✅ Crypto news feed (24 articles pulled from Finnhub crypto + CoinGecko)
- ✅ Web dashboard (Flask, dark theme, mobile-friendly, port 5555)
- ✅ Windows auto-start on boot (VBS in Startup folder)
- ✅ Cloudflare tunnel for remote phone access (cloudflared.exe downloaded)

## What Needs Attention
- **Cloudflare tunnel URL is temporary** — changes each restart. The free "quick tunnel" gives a random URL like `https://random-words.trycloudflare.com`. For a permanent URL, need a Cloudflare account + domain.
- **Tunnel DNS error** — the tunnel URL stopped working (likely laptop went to sleep). Need to restart scheduler.
- **Robinhood MCP** — Added `robinhood-trading` MCP server to Claude Code config but authentication never completed. The MCP shows "Needs authentication" in `claude mcp list`. The `/mcp` command just dismisses without showing auth flow. Portfolio data was pulled by scraping Robinhood website via Playwright browser instead.
- **No stocks tracked yet** — user is open to stock suggestions but currently only tracking XRP and DOGE crypto.
- **Termux sync** — user also has a Termux setup on their Android phone from a prior Claude web chat. Code hasn't been synced between Windows and Termux yet.

## How to Run
```bash
# Start everything (alerts + dashboard + tunnel + scheduled digest)
cd C:\Users\BmAn\Desktop\robnhud
python scheduler.py

# Or individual commands:
python prices.py              # See live prices
python alerts.py --list       # See alert rules
python alerts.py --pct BTC 3  # Add 3% alerts for new coin
python alerts.py --check      # Check alerts once
python news.py                # Read latest news
python digest.py --preview    # Preview email digest
python digest.py --send       # Send digest now
python dashboard.py           # Run dashboard only
python holdings.py --sync     # Re-import positions.txt
```

## Alert Configuration
- XRP above $1.5244 (3% up from $1.48 reference) — armed
- XRP below $1.4356 (3% down from $1.48 reference) — armed
- DOGE above $0.0915 (3% up from $0.0889 reference) — armed
- DOGE below $0.0862 (3% down from $0.0889 reference) — armed
- Cooldown: 45 minutes between repeated alerts
- Hysteresis: 0.75% (prevents alert spam at threshold boundaries)
- Alerts auto-recalibrate to current price when re-arming

## User Info
- Name: Brandon Westerhoff
- Email: branwesterhoff@gmail.com
- Robinhood account: Individual #609001318 (username bestWestern)
- Uses Termux on Android phone + Windows 11 laptop
- Claude Pro subscriber (Opus 5)
- Project name on Claude web: "robnhud"
- Prior Claude web chats in project: "Custom investment alerts and portfolio dashboard", "Setting up and opening in Termux", "Finding Brandon in green text"
