#!/data/data/com.termux/files/usr/bin/bash
# CRYPTEK VIGIL — Termux Setup Script
# Run this on your Android phone in Termux

set -e

echo "═══════════════════════════════════════════════"
echo "  CRYPTEK VIGIL v2.0 — Termux Installation"
echo "═══════════════════════════════════════════════"

# Step 1: Install dependencies
echo "[1/6] Installing packages..."
pkg update -y
pkg install -y git rust openssl

# Step 2: Clone repo
echo "[2/6] Cloning repository..."
cd ~
if [ -d "robnhud" ]; then
    echo "  robnhud/ already exists — pulling latest..."
    cd robnhud
    git pull origin master
else
    git clone https://github.com/bbwesti/robnhud.git
    cd robnhud
fi

# Step 3: Create .env
echo "[3/6] Setting up .env..."
if [ ! -f .env ]; then
    cat > .env << 'ENVEOF'
FINNHUB_KEY=da6gqv9r01qna3celpvgda6gqv9r01qna3celq00
NTFY_TOPIC=bw-mkt-8f3k2q9x
GMAIL_ADDRESS=branwesterhoff@gmail.com
GMAIL_APP_PASSWORD=gmwprzylbnvaxpiu
DIGEST_TO=branwesterhoff@gmail.com
SEC_USER_AGENT="Brandon Westerhoff branwesterhoff@gmail.com"
TIMEZONE=America/Los_Angeles
POLL_SECONDS=60
ALERT_COOLDOWN_MINUTES=45
HYSTERESIS_PCT=0.75
ENVEOF
    echo "  .env created"
else
    echo "  .env already exists — skipping"
fi

# Step 4: Create positions.txt
echo "[4/6] Setting up positions..."
if [ ! -f positions.txt ]; then
    cat > positions.txt << 'POSEOF'
XRP,crypto,434.841,1.05
DOGE,crypto,1.72,0.15
POSEOF
    echo "  positions.txt created"
else
    echo "  positions.txt already exists — skipping"
fi

# Step 5: Build
echo "[5/6] Building (this takes a few minutes on first run)..."
cargo build --release -p cryptek-vigil 2>&1

# Step 6: Verify
echo "[6/6] Verifying..."
./target/release/cryptek-vigil &
VIGIL_PID=$!
sleep 5

if curl -s http://localhost:5555/api/data > /dev/null 2>&1; then
    echo ""
    echo "═══════════════════════════════════════════════"
    echo "  CRYPTEK VIGIL is ONLINE"
    echo "  Dashboard: http://localhost:5555"
    echo "═══════════════════════════════════════════════"
else
    echo "  Warning: Dashboard not responding yet (may need more time)"
fi

kill $VIGIL_PID 2>/dev/null

echo ""
echo "To run: cd ~/robnhud && ./target/release/cryptek-vigil"
echo "To update: cd ~/robnhud && git pull && cargo build --release -p cryptek-vigil"
