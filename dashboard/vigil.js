/* ============================================================
   CRYPTEK VIGIL — vigil.js
   Dynasty Wealth Monitoring Protocol
   ============================================================ */

'use strict';

// ── Security Utilities (Agent 12) ───────────────────────────
function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = String(str);
  return div.innerHTML;
}

// ── Constants ─────────────────────────────────────────────────
const REFRESH_INTERVAL_MS = 60_000;
const PARTICLE_COUNT      = 60;
const PARTICLE_SPEED      = 0.35;
const CONNECTION_DIST     = 110;
const NECRON_GLYPHS       = ['◆', '◇', '⬡', '⬢', '⎔', '◈', '⬟', '⬠', '⎗', '⎘'];
const GLYPH_COUNT         = 18;

// ── State ─────────────────────────────────────────────────────
let countdownSecs     = 60;
let countdownTimer    = null;
let particles         = [];
let canvas            = null;
let ctx               = null;
let animFrameId       = null;
let lastData          = null;
let historyData       = {};   // { [symbol]: [price, ...] }
let portfolioHistory  = [];   // [ totalValue, ... ]
let holdingsSortKey   = 'value';
let holdingsExpanded  = false;
const HISTORY_INTERVAL_MS = 300_000; // 5 minutes
let govVerifiedCount = 0;
let govReplayCount = 0;

// ── Connection Health State (Agent 2 — backoff + Agent 11 — visibility) ──
let lastSuccessfulFetch   = null;
let fetchDataInFlight     = false;
let fetchQBInFlight       = false;
let consecutiveFailures   = 0;
const BACKOFF_BASE_MS     = REFRESH_INTERVAL_MS;
const BACKOFF_MAX_MS      = 300_000;
let fetchDataTimerId      = null;
let fetchQBTimerId        = null;
let currentBackoffMs      = BACKOFF_BASE_MS;

// ── Countdown drift fix (Agent 8) ──────────────────────────
let countdownEpoch = null; // absolute timestamp for countdown reference
let countdownTotal = 60;

// ── Holdings helpers (Agent 7) ──────────────────────────────
function safeNum(v) {
  const n = Number(v);
  return isFinite(n) ? n : 0;
}

function plPct(value, pl) {
  const cost = safeNum(value) - safeNum(pl);
  if (cost === 0) return 0;
  return (safeNum(pl) / cost) * 100;
}

function safeSortCompare(a, b, key) {
  return safeNum(b[key]) - safeNum(a[key]);
}

function truncSymbol(sym, max) {
  max = max || 12;
  if (!sym) return '--';
  return sym.length > max ? sym.slice(0, max - 1) + '\u2026' : sym;
}

// ── Formatters ────────────────────────────────────────────────
const fmt = {
  currency: v => {
    const n = Number(v);
    if (isNaN(n) || !isFinite(n)) return '--';
    return '$' + Math.abs(n).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  },
  pct: v => {
    const n = Number(v);
    if (isNaN(n) || !isFinite(n)) return '--';
    return (n >= 0 ? '+' : '') + n.toFixed(2) + '%';
  },
  qty: v => {
    const n = Number(v);
    if (isNaN(n) || !isFinite(n)) return '--';
    return n.toLocaleString('en-US', { minimumFractionDigits: 3, maximumFractionDigits: 6 });
  },
  short: s => {
    if (!s) return '--';
    return s.length > 20 ? s.slice(0, 10) + '\u2026' + s.slice(-6) : s;
  },
  sign: v => (Number(v) >= 0 ? '+' : '-'),
};

function signClass(v) {
  const n = Number(v);
  if (isNaN(n)) return '';
  return n >= 0 ? 'positive' : 'negative';
}

function dirClass(v) {
  const n = Number(v);
  if (isNaN(n)) return '';
  return n >= 0 ? 'up' : 'down';
}

// ── MarketScope helpers (Agent 5) ───────────────────────────
function safeStat(val) {
  var n = Number(val || 0);
  return isFinite(n) ? n.toFixed(2) : '--';
}

function clearMsResults() {
  var statsEl = document.getElementById('ms-stats');
  if (statsEl) { statsEl.innerHTML = ''; statsEl.style.display = 'none'; }
  var scenEl = document.getElementById('ms-scenarios');
  if (scenEl) scenEl.innerHTML = '';
  var auditEl = document.getElementById('ms-audit');
  if (auditEl) { auditEl.innerHTML = ''; auditEl.style.display = 'none'; }
  var discEl = document.getElementById('ms-disclaimer');
  if (discEl) discEl.style.display = 'none';
}

// ── Sparkline Renderer (Agent 3 — HiDPI) ─────────────────────
function drawSparkline(canvasEl, prices, color) {
  color = color || '#00ff66';
  if (!canvasEl || !prices || prices.length < 2) return;
  const dpr = window.devicePixelRatio || 1;

  // Use CSS display size from getBoundingClientRect for HiDPI support
  const rect = canvasEl.getBoundingClientRect();
  const w = rect.width;
  const h = rect.height;
  if (w === 0 || h === 0) return;

  // Scale the canvas backing store for sharp rendering on HiDPI displays
  canvasEl.width  = Math.round(w * dpr);
  canvasEl.height = Math.round(h * dpr);

  const c = canvasEl.getContext('2d');
  c.scale(dpr, dpr);
  c.clearRect(0, 0, w, h);

  const min = Math.min.apply(null, prices);
  const max = Math.max.apply(null, prices);
  const range = max - min || 1;

  c.beginPath();
  c.strokeStyle = color;
  c.lineWidth = 1.5;
  c.lineJoin = 'round';

  prices.forEach(function(p, i) {
    const x = (i / (prices.length - 1)) * w;
    const y = h - ((p - min) / range) * (h - 4) - 2;
    if (i === 0) c.moveTo(x, y);
    else c.lineTo(x, y);
  });
  c.stroke();

  // Gradient fill under the line
  const isRgb = color.startsWith('rgb(');
  let fillColor;
  if (isRgb) {
    fillColor = color.replace(')', ',0.12)').replace('rgb', 'rgba');
  } else if (color.startsWith('#')) {
    const r = parseInt(color.slice(1,3), 16);
    const g = parseInt(color.slice(3,5), 16);
    const b = parseInt(color.slice(5,7), 16);
    fillColor = 'rgba(' + r + ',' + g + ',' + b + ',0.12)';
  } else {
    fillColor = 'rgba(0,255,102,0.12)';
  }
  const grad = c.createLinearGradient(0, 0, 0, h);
  grad.addColorStop(0, fillColor);
  grad.addColorStop(1, 'transparent');
  // Close path for fill
  c.beginPath();
  prices.forEach(function(p, i) {
    const x = (i / (prices.length - 1)) * w;
    const y = h - ((p - min) / range) * (h - 4) - 2;
    if (i === 0) c.moveTo(x, y);
    else c.lineTo(x, y);
  });
  c.lineTo(w, h);
  c.lineTo(0, h);
  c.closePath();
  c.fillStyle = grad;
  c.fill();
}

// Attach tooltip behavior to a sparkline canvas
function attachSparklineTooltip(canvasEl, prices, label) {
  const tooltip = document.getElementById('sparkline-tooltip');
  if (!tooltip || !prices || prices.length < 2) return;

  canvasEl._tooltipData = { prices, label };

  if (canvasEl._tooltipAttached) return;
  canvasEl._tooltipAttached = true;

  canvasEl.addEventListener('mousemove', function(e) {
    const data = canvasEl._tooltipData;
    if (!data) return;
    const rect = canvasEl.getBoundingClientRect();
    const relX = e.clientX - rect.left;
    const idx = Math.min(data.prices.length - 1, Math.max(0, Math.round((relX / rect.width) * (data.prices.length - 1))));
    const price = data.prices[idx];
    tooltip.textContent = (data.label ? data.label + ': ' : '') + fmt.currency(price);
    tooltip.style.display = 'block';
    tooltip.style.left = (e.clientX + 10) + 'px';
    tooltip.style.top  = (e.clientY - 24) + 'px';
  });

  canvasEl.addEventListener('mouseleave', function() {
    tooltip.style.display = 'none';
  });
}

// ── History Fetch ─────────────────────────────────────────────
async function fetchHistory() {
  try {
    const res = await fetch('/api/history');
    if (!res.ok) return;
    const data = await res.json();
    const rows = data.history || [];

    const grouped = {};
    rows.forEach(function(r) {
      if (!grouped[r.symbol]) grouped[r.symbol] = [];
      grouped[r.symbol].unshift(r.price);
    });
    historyData = grouped;

    if (lastData && lastData.holdings) {
      const qtyMap = {};
      lastData.holdings.forEach(function(h) { qtyMap[h.symbol] = h.qty; });
      const heldSymbols = lastData.holdings.map(function(h) { return h.symbol; });
      const lengths = heldSymbols.map(function(s) { return (grouped[s] || []).length; }).filter(function(l) { return l > 0; });
      if (lengths.length > 0) {
        const minLen = Math.min.apply(null, lengths);
        const portVals = [];
        for (let i = 0; i < minLen; i++) {
          let total = 0;
          heldSymbols.forEach(function(s) {
            const arr = grouped[s] || [];
            const offset = arr.length - minLen;
            const price = arr[offset + i] || 0;
            total += price * (qtyMap[s] || 0);
          });
          portVals.push(total);
        }
        portfolioHistory = portVals;
        drawVaultChart();
      }
    }

    if (lastData && lastData.holdings) {
      redrawHoldingSparklines();
    }
    if (lastData && lastData.indices) {
      redrawIndexSparklines();
    }
  } catch(err) {
    console.warn('[VIGIL] history fetch error:', err);
  }
}

function drawVaultChart() {
  const el = document.getElementById('vault-chart');
  if (!el || portfolioHistory.length < 2) return;
  const trend = portfolioHistory[portfolioHistory.length - 1] - portfolioHistory[0];
  const color = trend >= 0 ? '#00ff66' : '#ff3333';
  drawSparkline(el, portfolioHistory, color);
  attachSparklineTooltip(el, portfolioHistory, 'VAULT');
}

function redrawHoldingSparklines() {
  const list = document.getElementById('holdings-list');
  if (!list) return;
  list.querySelectorAll('.holding-row').forEach(function(row) {
    const sym = row.dataset.symbol;
    const prices = historyData[sym];
    const canvasEl = row.querySelector('canvas.sparkline');
    if (canvasEl && prices && prices.length >= 2) {
      const trend = prices[prices.length - 1] - prices[0];
      drawSparkline(canvasEl, prices, trend >= 0 ? '#00ff66' : '#ff3333');
    }
  });
}

function redrawIndexSparklines() {
  const grid = document.getElementById('indices-grid');
  if (!grid) return;
  grid.querySelectorAll('.index-card').forEach(function(card) {
    const sym = card.dataset.symbol;
    const prices = historyData[sym];
    const canvasEl = card.querySelector('canvas.sparkline');
    if (canvasEl && prices && prices.length >= 2) {
      const trend = prices[prices.length - 1] - prices[0];
      drawSparkline(canvasEl, prices, trend >= 0 ? '#00ff66' : '#ff3333');
    }
  });
}

// ── Clock (Agent 2 — staleness detection) ────────────────────
function updateClock() {
  const now = new Date();
  const h   = String(now.getHours()).padStart(2, '0');
  const m   = String(now.getMinutes()).padStart(2, '0');
  const s   = String(now.getSeconds()).padStart(2, '0');

  const elTime = document.getElementById('clock-time');
  if (elTime) elTime.textContent = h + ':' + m + ':' + s;

  const cycleNum = Math.floor(Date.now() / 60000);
  const elCycle  = document.getElementById('clock-cycle');
  if (elCycle) elCycle.textContent = 'CYCLE: ' + cycleNum.toString().padStart(8, '0');

  // Data Staleness Detection (Agent 2)
  if (lastSuccessfulFetch !== null) {
    const ageMs = Date.now() - lastSuccessfulFetch;
    const dot = document.querySelector('.status-dot');
    if (dot) {
      if (ageMs > 300_000) {
        dot.style.background = '#ff3333';
      } else if (ageMs > 120_000) {
        dot.style.background = '#ffaa00';
      }
    }
  }
}

// ── Countdown Bar (Agent 8 — drift fix + Agent 11 — transform) ──
function resetCountdown() {
  const delaySecs = Math.round(currentBackoffMs / 1000);
  countdownTotal = delaySecs;
  countdownSecs  = delaySecs;
  countdownEpoch = Date.now();
  renderCountdown();
}

function renderCountdown() {
  const fill = document.getElementById('countdown-fill');
  if (!fill) return;
  const denom = countdownTotal > 0 ? countdownTotal : 60;
  const pct = Math.max(0, Math.min(1, countdownSecs / denom));
  // Use transform:scaleX for GPU-accelerated rendering (Agent 11)
  fill.style.transform = 'scaleX(' + pct + ')';
}

function tickCountdown() {
  // Use absolute time to avoid drift (Agent 8)
  if (countdownEpoch !== null) {
    const elapsed = Math.floor((Date.now() - countdownEpoch) / 1000);
    countdownSecs = Math.max(0, countdownTotal - elapsed);
  } else if (countdownSecs > 0) {
    countdownSecs--;
  }
  renderCountdown();
}

// ── Data Fetch (Agent 2 — backoff, Agent 11 — in-flight guard) ──
async function fetchData() {
  if (fetchDataInFlight) return;
  fetchDataInFlight = true;

  try {
    const res  = await fetch('/api/data');
    if (!res.ok) throw new Error('HTTP ' + res.status);
    const data = await res.json();
    lastData   = data;

    updatePortfolio(data.portfolio);
    updateHoldings(data.holdings);
    updateRegime(data.regime);
    updateAlerts(data.alerts);
    updateWorm(data.worm);
    updateIndices(data.indices);
    updateScenarios(data.scenarios);
    updateStatus(data.status);

    lastSuccessfulFetch = Date.now();
    consecutiveFailures = 0;
    currentBackoffMs    = BACKOFF_BASE_MS;

    resetCountdown();

    const dot = document.querySelector('.status-dot');
    if (dot) dot.style.background = '#00ff66';

    dismissConnectionError();

    if (Object.keys(historyData).length > 0) {
      redrawHoldingSparklines();
      redrawIndexSparklines();
      drawVaultChart();
    }
  } catch (err) {
    console.error('[VIGIL] fetch error:', err);
    consecutiveFailures++;
    const dot = document.querySelector('.status-dot');
    if (dot) dot.style.background = '#ff3333';
    flashError('TRANSMISSION INTERRUPTED \u2014 ' + err.message);
  } finally {
    fetchDataInFlight = false;
    scheduleFetchData();
  }
}

function flashMessage(msg, type) {
  type = type || 'error';
  const el = document.getElementById('error-banner');
  if (!el) return;
  el.textContent = msg;
  el.className = 'error-banner visible' + (type === 'info' ? ' info' : '');
  el.style.display = 'block';

  // Info messages auto-dismiss. Error banners persist until recovery (Agent 2)
  if (type === 'info') {
    setTimeout(function() { el.style.display = 'none'; el.className = 'error-banner'; }, 3000);
  }
}

function flashError(msg) {
  flashMessage(msg, 'error');
}

function dismissConnectionError() {
  const el = document.getElementById('error-banner');
  if (!el) return;
  if (el.classList.contains('visible') && !el.classList.contains('info')) {
    el.style.display = 'none';
    el.className = 'error-banner';
  }
}

// ── DOM Updaters ──────────────────────────────────────────────
function updatePortfolio(p) {
  if (!p) return;
  const elVal = document.getElementById('vault-value');
  if (elVal) elVal.textContent = fmt.currency(p.total_value);

  const elPl  = document.getElementById('vault-pl');
  if (elPl) {
    const sign = safeNum(p.total_pl) >= 0 ? '+' : '-';
    elPl.textContent = sign + fmt.currency(p.total_pl) + ' (' + fmt.pct(p.total_pl_pct) + ')';
    elPl.className   = 'vault-pl ' + signClass(p.total_pl);
  }
}

function updateHoldings(holdings) {
  if (!Array.isArray(holdings)) return;
  const list = document.getElementById('holdings-list');
  if (!list) return;

  // Save scroll position (Agent 7)
  const scrollTop = list.scrollTop;

  list.innerHTML = '';

  // Empty state (Agent 7)
  if (holdings.length === 0) {
    list.innerHTML = '<div class="empty-state">No holdings in vault</div>';
    return;
  }

  // Sort (Agent 7 — use safeSortCompare for NaN safety)
  var sortKey = holdingsSortKey === 'price' ? 'price' : 'value';
  const sorted = holdings.slice().sort(function(a, b) { return safeSortCompare(a, b, sortKey); });

  sorted.forEach(function(h) {
    const row = document.createElement('div');
    row.className = 'holding-row';
    row.dataset.symbol = h.symbol;

    const chgClass = dirClass(h.change_pct);
    const plSign   = safeNum(h.pl) >= 0 ? '+' : '-';
    const plClass  = signClass(h.pl);
    const sym = escapeHtml(truncSymbol(h.symbol));

    if (holdingsSortKey === 'value') {
      row.innerHTML = '<div>' +
          '<div class="holding-symbol">' + sym + '</div>' +
          '<div class="holding-class">' + escapeHtml(h.asset_class || '') + '</div>' +
        '</div>' +
        '<div class="holding-right-stack">' +
          '<div class="holding-value-big">' + escapeHtml(fmt.currency(h.value)) + '</div>' +
          '<div class="holding-pl ' + plClass + '">' + escapeHtml(plSign + fmt.currency(h.pl)) + ' (' + escapeHtml(fmt.pct(plPct(h.value, h.pl))) + ')</div>' +
        '</div>' +
        '<div class="holding-sparkline-wrap">' +
          '<canvas class="sparkline" aria-label="' + escapeHtml(h.symbol) + ' value history"></canvas>' +
        '</div>' +
        '<div class="holding-sub-row">' +
          '<span class="sub-label">QTY</span><span class="sub-val">' + escapeHtml(fmt.qty(h.qty)) + '</span>' +
          '<span class="sub-sep">|</span>' +
          '<span class="sub-label">PRICE</span><span class="sub-val">' + escapeHtml(fmt.currency(h.price)) + '</span>' +
        '</div>';
    } else if (holdingsSortKey === 'price') {
      row.innerHTML = '<div>' +
          '<div class="holding-symbol">' + sym + '</div>' +
          '<div class="holding-class">' + escapeHtml(h.asset_class || '') + '</div>' +
        '</div>' +
        '<div class="holding-right-stack">' +
          '<div class="holding-price-big">' + escapeHtml(fmt.currency(h.price)) + '</div>' +
          '<div class="holding-change-big ' + chgClass + '">' + escapeHtml(fmt.pct(h.change_pct)) + ' 24h</div>' +
        '</div>' +
        '<div class="holding-sparkline-wrap">' +
          '<canvas class="sparkline" aria-label="' + escapeHtml(h.symbol) + ' price history"></canvas>' +
        '</div>' +
        '<div class="holding-sub-row">' +
          '<span class="sub-label">CHG</span><span class="sub-val ' + chgClass + '">' + escapeHtml(fmt.pct(h.change_pct)) + '</span>' +
          '<span class="sub-sep">|</span>' +
          '<span class="sub-label">VALUE</span><span class="sub-val">' + escapeHtml(fmt.currency(h.value)) + '</span>' +
        '</div>';
    } else if (holdingsSortKey === 'details') {
      row.innerHTML = '<div>' +
          '<div class="holding-symbol">' + sym + '</div>' +
          '<div class="holding-class">' + escapeHtml(h.asset_class || '') + '</div>' +
        '</div>' +
        '<div class="holding-right-stack">' +
          '<div class="holding-price">' + escapeHtml(fmt.currency(h.price)) + '</div>' +
          '<div class="holding-change ' + chgClass + '">' + escapeHtml(fmt.pct(h.change_pct)) + '</div>' +
        '</div>' +
        '<div class="holding-detail-always">' +
          '<div class="detail-grid">' +
            '<div><div class="detail-label">QTY</div><div class="detail-val">' + escapeHtml(fmt.qty(h.qty)) + '</div></div>' +
            '<div><div class="detail-label">AVG COST</div><div class="detail-val">' + escapeHtml(fmt.currency(h.avg_cost)) + '</div></div>' +
            '<div><div class="detail-label">VALUE</div><div class="detail-val">' + escapeHtml(fmt.currency(h.value)) + '</div></div>' +
            '<div><div class="detail-label">P/L</div><div class="detail-val ' + plClass + '">' + escapeHtml(plSign + fmt.currency(h.pl)) + '</div></div>' +
          '</div>' +
        '</div>' +
        '<div class="holding-sparkline-wrap">' +
          '<canvas class="sparkline" aria-label="' + escapeHtml(h.symbol) + ' price history"></canvas>' +
        '</div>';
    }

    const prices = historyData[h.symbol];
    const canvasEl = row.querySelector('canvas.sparkline');
    if (canvasEl && prices && prices.length >= 2) {
      const trend = prices[prices.length - 1] - prices[0];
      drawSparkline(canvasEl, prices, trend >= 0 ? '#00ff66' : '#ff3333');
      attachSparklineTooltip(canvasEl, prices, h.symbol);
    }

    list.appendChild(row);
  });

  // Restore scroll position (Agent 7)
  list.scrollTop = scrollTop;
}

function updateRegime(regime) {
  if (!regime) return;
  const badge = document.getElementById('regime-badge');
  const desc  = document.getElementById('regime-desc');
  if (!badge) return;

  const cls = regime.classification.toLowerCase();
  let badgeClass = 'neutral';
  if (cls.includes('bull'))       badgeClass = 'bull';
  else if (cls.includes('bear'))  badgeClass = 'bear';
  else if (cls.includes('trans')) badgeClass = 'transition';

  badge.textContent = regime.classification;
  badge.className   = 'regime-badge ' + badgeClass;
  if (desc) desc.textContent = regime.description || '';
}

function updateAlerts(alerts) {
  if (!Array.isArray(alerts)) return;
  const list = document.getElementById('alerts-list');
  if (!list) return;

  list.innerHTML = '';

  // Empty state (Agent 8)
  if (alerts.length === 0) {
    list.innerHTML = '<div class="alert-row inactive"><span class="alert-dot"></span><span class="alert-symbol">NO ALERTS</span></div>';
    return;
  }

  alerts.forEach(function(a) {
    var statusRaw = ((a.status || '') + '').toLowerCase() || 'inactive';
    var rowClass = 'inactive';
    if (statusRaw === 'armed')                                    rowClass = 'armed';
    if (statusRaw === 'triggered' || statusRaw === 'fired')       rowClass = 'triggered';
    if (statusRaw === 'cooldown')                                 rowClass = 'cooldown';

    var dirText = a.last_direction ? ' \u2014 ' + escapeHtml(String(a.last_direction).toUpperCase()) : '';
    // Invalid date guard (Agent 8)
    var timeText = '';
    if (a.last_triggered) {
      var d = new Date(a.last_triggered);
      if (!isNaN(d.getTime())) {
        timeText = ' ' + d.toLocaleTimeString();
      }
    }

    var row = document.createElement('div');
    row.className = 'alert-row ' + rowClass;
    row.innerHTML = '<div class="alert-dot"></div>' +
      '<span class="alert-symbol">' + escapeHtml(a.symbol || '--') + '</span>' +
      '<span class="alert-status">' + escapeHtml(a.status || 'inactive') + dirText + timeText + '</span>';
    list.appendChild(row);
  });
}

function updateWorm(worm) {
  // Null-safe (Agent 6)
  if (!worm) worm = {};

  const elBlocks = document.getElementById('worm-blocks');
  const elValid  = document.getElementById('worm-valid');
  const elCount  = document.getElementById('worm-count');
  const elHash   = document.getElementById('worm-hash');
  const elType   = document.getElementById('worm-type');

  if (elValid) {
    elValid.textContent = worm.valid ? 'VERIFIED \u25C6' : 'BREACH DETECTED';
    elValid.className   = 'worm-val ' + (worm.valid ? 'valid' : 'invalid');
  }
  if (elCount) elCount.textContent = worm.block_count != null ? worm.block_count : '--';
  if (elHash)  elHash.textContent  = worm.last_hash  ? fmt.short(worm.last_hash) : '--';
  if (elType)  elType.textContent  = (worm.last_type || '--').toUpperCase();

  const elSigs = document.getElementById('worm-sigs');
  const elAlgo = document.getElementById('worm-algo');

  if (elSigs) {
    const sigsValid = worm.signatures_valid;
    elSigs.textContent = sigsValid ? 'VERIFIED \u25C6' : (sigsValid === false ? 'INVALID' : '\u2014');
    elSigs.className = 'worm-val ' + (sigsValid ? 'valid' : (sigsValid === false ? 'invalid' : ''));
  }
  if (elAlgo) {
    elAlgo.textContent = (worm.hash_algo || 'sha256').toUpperCase();
  }

  if (elBlocks) {
    elBlocks.innerHTML = '';
    const n = Math.min(Number(worm.block_count) || 0, 20);
    for (let i = 0; i < n; i++) {
      const b = document.createElement('div');
      b.className = 'chain-block';
      b.style.animationDelay = (i * 0.1) + 's';
      elBlocks.appendChild(b);
    }
  }
}

function updateIndices(indices) {
  if (!Array.isArray(indices)) return;
  const grid = document.getElementById('indices-grid');
  if (!grid) return;

  grid.innerHTML = '';

  if (indices.length === 0) {
    grid.innerHTML = '<div class="empty-state">No indices configured</div>';
    return;
  }

  indices.forEach(function(idx) {
    const card = document.createElement('div');
    card.className = 'index-card';
    card.dataset.symbol = idx.symbol;
    const chg = dirClass(idx.change_pct);
    card.innerHTML =
      '<div class="index-symbol">' + escapeHtml(idx.symbol) + '</div>' +
      '<div class="index-name">' + escapeHtml(idx.name || '') + '</div>' +
      '<div class="index-price">' + escapeHtml(fmt.currency(idx.price)) + '</div>' +
      '<div class="index-change ' + chg + '">' + escapeHtml(fmt.pct(idx.change_pct)) + '</div>' +
      '<div class="index-sparkline-wrap">' +
        '<canvas class="sparkline" aria-label="' + escapeHtml(idx.symbol) + ' history"></canvas>' +
      '</div>';

    const prices = historyData[idx.symbol];
    const canvasEl = card.querySelector('canvas.sparkline');
    if (canvasEl && prices && prices.length >= 2) {
      const trend = prices[prices.length - 1] - prices[0];
      drawSparkline(canvasEl, prices, trend >= 0 ? '#00ff66' : '#ff3333');
      attachSparklineTooltip(canvasEl, prices, idx.symbol);
    }

    grid.appendChild(card);
  });
}

function updateScenarios(scenarios) {
  if (!Array.isArray(scenarios)) return;
  const list = document.getElementById('scenarios-list');
  if (!list) return;

  list.innerHTML = '';

  if (scenarios.length === 0) {
    list.innerHTML = '<div class="empty-state">Insufficient price history for projections</div>';
    return;
  }

  scenarios.forEach(function(sc) {
    const item = document.createElement('div');
    item.className = 'scenario-item';

    const bands = Array.isArray(sc.bands) ? sc.bands : [];

    // Empty bands message (Agent 5)
    if (bands.length === 0) {
      item.innerHTML = '<div class="scenario-header"><div class="scenario-symbol">' + escapeHtml(sc.symbol) + '</div></div>' +
        '<div class="empty-state">No bands computed</div>';
      list.appendChild(item);
      return;
    }

    const vals = bands.map(function(b) {
      const r = Array.isArray(b.range) ? b.range : [];
      return Math.max(Math.abs(Number(r[0]) || 0), Math.abs(Number(r[1]) || 0));
    }).filter(function(v) { return v > 0; });
    const maxVal = vals.length ? Math.max.apply(null, vals) : 1;

    const bandsHtml = bands.map(function(b) {
      const r = Array.isArray(b.range) ? b.range : [0, 0];
      const lo = Number(r[0]) || 0;
      const hi = Number(r[1]) || 0;
      // Defensive range check (Agent 5)
      if (!isFinite(lo) || !isFinite(hi)) return '';
      const mid = (lo + hi) / 2;
      const pctBar = maxVal > 0 ? ((Math.abs(mid) / maxVal) * 100).toFixed(1) : 0;
      return '<div class="scenario-band">' +
          '<div class="band-label">' + escapeHtml(b.label || b.scenario || '') + '</div>' +
          '<div class="band-bar-track"><div class="band-bar-fill" style="width:' + pctBar + '%"></div></div>' +
          '<div class="band-val">' + escapeHtml(fmt.currency(lo)) + ' \u2014 ' + escapeHtml(fmt.currency(hi)) + '</div>' +
          '<div class="band-pct">' + escapeHtml(b.probability || '') + '</div>' +
        '</div>';
    }).join('');

    item.innerHTML =
      '<div class="scenario-header">' +
        '<div class="scenario-symbol">' + escapeHtml(sc.symbol) + '</div>' +
        '<div class="scenario-hash">' + (sc.data_hash ? escapeHtml(fmt.short(sc.data_hash)) : '') + '</div>' +
      '</div>' +
      bandsHtml +
      (sc.disclaimer ? '<div class="scenario-disclaimer">' + escapeHtml(sc.disclaimer) + '</div>' : '');

    list.appendChild(item);
  });
}

function updateStatus(status) {
  if (!status) return;
  const elCore    = document.getElementById('status-vigil');
  const elDigest  = document.getElementById('status-digest');
  const elRefresh = document.getElementById('status-refresh');

  if (elCore)    elCore.textContent    = ((status.vigil_core || '') + '' || '--').toUpperCase();
  if (elDigest)  elDigest.textContent  = status.next_digest || '--';
  if (elRefresh) {
    const d = status.last_refresh ? new Date(status.last_refresh) : null;
    elRefresh.textContent = (d && !isNaN(d.getTime())) ? d.toLocaleTimeString() : '--';
  }
}

// ── QuantaBeta Fetch (Agent 2 — in-flight guard + Agent 12 — escapeHtml) ──
async function fetchQuantaBeta() {
    if (fetchQBInFlight) return;
    fetchQBInFlight = true;

    try {
        const res = await fetch('/api/quantabeta');
        if (!res.ok) throw new Error('HTTP ' + res.status);
        const data = await res.json();
        if (data.status === 'no_data') {
            document.getElementById('qb-status').textContent = 'STANDBY';
            return;
        }

        if (!Array.isArray(data.assets) || data.assets.length === 0) {
            document.getElementById('qb-status').textContent = 'NO ASSETS';
            return;
        }

        document.getElementById('qb-status').textContent = 'ACTIVE';
        document.getElementById('qb-status').style.color = '#00ff66';

        const allRam = data.assets.every(function(a) { return a.ramanujan_congruence_holds; });
        document.getElementById('qb-ramanujan').textContent = allRam ? 'VERIFIED' : 'VIOLATED';
        document.getElementById('qb-ramanujan').style.color = allRam ? '#00ff66' : '#ff3333';

        const avgEntropy = data.assets.reduce(function(s, a) { return s + a.entropy_point; }, 0) / data.assets.length;
        document.getElementById('qb-entropy').textContent = avgEntropy.toFixed(3);

        document.getElementById('qb-hecke-pairs').textContent = data.cross_correlations.length;

        const tbody = document.getElementById('qb-asset-tbody');
        tbody.innerHTML = '';
        for (var ai = 0; ai < data.assets.length; ai++) {
            var asset = data.assets[ai];
            var lastVol = asset.partition_volatility.length > 0
                ? asset.partition_volatility[asset.partition_volatility.length - 1].toFixed(4)
                : '\u2014';
            var complexityDisplay = asset.complexity_bound.length > 10 ? asset.complexity_bound.substring(0, 8) + '...' : asset.complexity_bound;
            var tr = document.createElement('tr');
            tr.innerHTML =
                '<td>' + escapeHtml(asset.symbol) + '</td>' +
                '<td>' + escapeHtml(lastVol) + '</td>' +
                '<td>[' + escapeHtml(asset.entropy_interval[0].toFixed(3)) + ', ' + escapeHtml(asset.entropy_interval[1].toFixed(3)) + ']</td>' +
                '<td>' + escapeHtml(complexityDisplay) + '</td>' +
                '<td class="' + (asset.ramanujan_congruence_holds ? 'qb-pass' : 'qb-fail') + '">' + (asset.ramanujan_congruence_holds ? 'PASS' : 'FAIL') + '</td>';
            tbody.appendChild(tr);
        }

        if (Array.isArray(data.cross_correlations) && data.cross_correlations.length > 0) {
            document.getElementById('qb-cross-section').style.display = 'block';
            const crossList = document.getElementById('qb-cross-list');
            crossList.innerHTML = '';
            for (var ci = 0; ci < data.cross_correlations.length; ci++) {
                var cc = data.cross_correlations[ci];
                var div = document.createElement('div');
                div.className = 'qb-cross-item';
                div.innerHTML =
                    '<span>' + escapeHtml(cc.asset_a) + ' \u2194 ' + escapeHtml(cc.asset_b) + '</span>' +
                    '<span>T_' + escapeHtml(String(cc.level)) + ': ' + escapeHtml(cc.hecke_correlation.toFixed(6)) + '</span>';
                crossList.appendChild(div);
            }
        }
    } catch (e) {
        console.warn('QuantaBeta fetch failed:', e);
    } finally {
        fetchQBInFlight = false;
        scheduleFetchQuantaBeta();
    }
}

// ── Governance (Agent 1 — better error parsing, Agent 6 — form, replay fix) ──
function initGovernanceForm() {
  var form = document.getElementById('gov-form');
  if (!form) return;
  form.addEventListener('submit', function(e) {
    e.preventDefault();
    if (!validateGovernanceForm()) return;
    var seal = {
      decision_id: document.getElementById('gov-decision-id').value.trim(),
      action: document.getElementById('gov-action').value.trim(),
      proposer: document.getElementById('gov-proposer').value.trim(),
      approvers: document.getElementById('gov-approvers').value.split(',').map(function(s) { return s.trim(); }).filter(Boolean),
      seal_hash: document.getElementById('gov-seal-hash').value.trim(),
      timestamp: document.getElementById('gov-timestamp').value.trim() || new Date().toISOString(),
      quorum_threshold: parseInt(document.getElementById('gov-quorum').value, 10) || 2,
    };
    verifyGovernanceSeal(seal);
  });
}

function validateGovernanceForm() {
  var id = document.getElementById('gov-decision-id');
  var hash = document.getElementById('gov-seal-hash');
  if (!id || !id.value.trim()) { flashError('Decision ID is required'); return false; }
  if (!hash || !hash.value.trim()) { flashError('Seal hash is required'); return false; }
  return true;
}

function displayGovernanceResult(result) {
  if (!result) return;
  var log = document.getElementById('gov-log');
  if (!log) return;
  if (log.querySelector('.empty-state')) log.innerHTML = '';
  var entry = document.createElement('div');
  entry.className = 'gov-log-entry ' + (result.seal_valid ? 'valid' : 'invalid');
  entry.textContent = result.reason || (result.seal_valid ? 'Seal verified' : 'Verification failed');
  log.prepend(entry);
}

function resetGovernanceForm() {
  var form = document.getElementById('gov-form');
  if (form) form.reset();
}

async function verifyGovernanceSeal(seal) {
    try {
        const res = await fetch('/api/governance/verify', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(seal),
        });
        // Parse JSON first for better error messages (Agent 1)
        const result = await res.json();
        if (!res.ok) {
            console.warn('Governance verify error:', result.error || res.status);
            flashError('Governance: ' + (result.error || 'HTTP ' + res.status));
            return null;
        }

        if (result.is_replay) {
            govReplayCount++;
        }
        if (result.seal_valid && result.quorum_met) {
            govVerifiedCount++;
        }

        var elVerified = document.getElementById('gov-verified-count');
        var elReplay = document.getElementById('gov-replay-count');
        if (elVerified) elVerified.textContent = govVerifiedCount;
        if (elReplay) elReplay.textContent = govReplayCount;

        displayGovernanceResult(result);
        resetGovernanceForm();

        return result;
    } catch (e) {
        console.warn('Governance verify failed:', e);
        return null;
    }
}

// ── Particle System (Agent 3 — HiDPI) ───────────────────────
function initParticles() {
  canvas = document.getElementById('particle-canvas');
  if (!canvas) return;
  ctx = canvas.getContext('2d');

  resizeCanvas();
  let resizeTimeout;
  window.addEventListener('resize', function() {
    clearTimeout(resizeTimeout);
    resizeTimeout = setTimeout(resizeCanvas, 150);
  });

  particles = [];
  for (let i = 0; i < PARTICLE_COUNT; i++) {
    particles.push(createParticle());
  }

  animateParticles();
}

function resizeCanvas() {
  if (!canvas) return;
  var dpr = window.devicePixelRatio || 1;
  var w = window.innerWidth;
  var h = window.innerHeight;
  canvas.width  = Math.round(w * dpr);
  canvas.height = Math.round(h * dpr);
  canvas.style.width = w + 'px';
  canvas.style.height = h + 'px';
  if (ctx) ctx.scale(dpr, dpr);
}

function createParticle(x, y) {
  const angle = Math.random() * Math.PI * 2;
  const speed = PARTICLE_SPEED * (0.3 + Math.random() * 0.7);
  // Use window dimensions instead of canvas dimensions (Agent 3)
  var w = window.innerWidth || 800;
  var h = window.innerHeight || 600;
  return {
    x:     x != null ? x : Math.random() * w,
    y:     y != null ? y : Math.random() * h,
    vx:    Math.cos(angle) * speed,
    vy:    Math.sin(angle) * speed,
    r:     1 + Math.random() * 1.5,
    alpha: 0.2 + Math.random() * 0.5,
    life:  Math.random(),
    speed: 0.005 + Math.random() * 0.01,
  };
}

function animateParticles() {
  if (!ctx || !canvas) return;
  var w = window.innerWidth;
  var h = window.innerHeight;

  ctx.clearRect(0, 0, w, h);

  particles.forEach(function(p) {
    p.x   += p.vx;
    p.y   += p.vy;
    p.life = (p.life + p.speed) % 1;

    const twinkle = 0.4 + 0.6 * Math.sin(p.life * Math.PI * 2);

    // Wrap around edges using window dimensions (Agent 3)
    if (p.x < -5)     p.x = w + 5;
    if (p.x > w + 5)  p.x = -5;
    if (p.y < -5)     p.y = h + 5;
    if (p.y > h + 5)  p.y = -5;

    ctx.beginPath();
    ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
    ctx.fillStyle = 'rgba(0,255,102,' + (p.alpha * twinkle) + ')';
    ctx.fill();
  });

  const connDistSq = CONNECTION_DIST * CONNECTION_DIST;
  for (let i = 0; i < particles.length; i++) {
    for (let j = i + 1; j < particles.length; j++) {
      const dx     = particles[i].x - particles[j].x;
      const dy     = particles[i].y - particles[j].y;
      const distSq = dx * dx + dy * dy;

      if (distSq < connDistSq) {
        const dist  = Math.sqrt(distSq);
        const alpha = (1 - dist / CONNECTION_DIST) * 0.12;
        ctx.beginPath();
        ctx.moveTo(particles[i].x, particles[i].y);
        ctx.lineTo(particles[j].x, particles[j].y);
        ctx.strokeStyle = 'rgba(0,255,102,' + alpha + ')';
        ctx.lineWidth   = 0.6;
        ctx.stroke();
      }
    }
  }

  animFrameId = requestAnimationFrame(animateParticles);
}

// ── Floating Necron Glyphs ────────────────────────────────────
function initGlyphs() {
  const layer = document.getElementById('glyph-layer');
  if (!layer) return;

  for (let i = 0; i < GLYPH_COUNT; i++) {
    const g = document.createElement('span');
    g.className = 'necron-glyph';
    g.textContent = NECRON_GLYPHS[i % NECRON_GLYPHS.length];

    const dur   = 10 + Math.random() * 14;
    const delay = -(Math.random() * dur);
    const left  = Math.random() * 95;

    g.style.setProperty('--dur',   dur + 's');
    g.style.setProperty('--delay', delay + 's');
    g.style.left     = left + '%';
    g.style.fontSize = (12 + Math.random() * 14) + 'px';

    layer.appendChild(g);
  }
}

// ── Keyboard Shortcuts ────────────────────────────────────────
function initKeyboardShortcuts() {
  document.addEventListener('keydown', function(e) {
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;

    const key = e.key.toUpperCase();

    if (key === 'R') {
      // Cancel pending scheduled fetches to avoid double-fire (Agent 2)
      if (fetchDataTimerId !== null) { clearTimeout(fetchDataTimerId); fetchDataTimerId = null; }
      if (fetchQBTimerId !== null) { clearTimeout(fetchQBTimerId); fetchQBTimerId = null; }
      fetchData();
      fetchQuantaBeta();
      fetchHistory();
      flashMessage('MANUAL REFRESH INITIATED', 'info');
    }

    if (key === '1') switchSection('command');
    if (key === '2') switchSection('marketscope');
    if (key === '3') switchSection('archives');
  });
}

// ── Sort Controls ─────────────────────────────────────────────
function initSortControls() {
  const bar = document.getElementById('holdings-sort-bar');
  if (!bar) return;
  bar.addEventListener('click', function(e) {
    const btn = e.target.closest('.sort-btn');
    if (!btn) return;
    const key = btn.dataset.sort;
    holdingsSortKey = key;
    bar.querySelectorAll('.sort-btn').forEach(function(b) { b.classList.remove('active'); });
    btn.classList.add('active');
    if (lastData && lastData.holdings) {
      updateHoldings(lastData.holdings);
    }
  });
}

// ── Fetch Scheduling with Exponential Backoff (Agent 2) ─────
function getBackoffDelay() {
  if (consecutiveFailures === 0) return BACKOFF_BASE_MS;
  var delay = Math.min(
    BACKOFF_BASE_MS * Math.pow(2, consecutiveFailures),
    BACKOFF_MAX_MS
  );
  currentBackoffMs = delay;
  return delay;
}

function scheduleFetchData() {
  if (fetchDataTimerId !== null) {
    clearTimeout(fetchDataTimerId);
  }
  var delay = getBackoffDelay();
  fetchDataTimerId = setTimeout(fetchData, delay);
  var delaySecs = Math.round(delay / 1000);
  countdownTotal = delaySecs;
  countdownSecs  = delaySecs;
  countdownEpoch = Date.now();
  renderCountdown();
}

function scheduleFetchQuantaBeta() {
  if (fetchQBTimerId !== null) {
    clearTimeout(fetchQBTimerId);
  }
  var delay = getBackoffDelay();
  fetchQBTimerId = setTimeout(fetchQuantaBeta, delay);
}

// ── Visibility Handler (Agent 11) ───────────────────────────
function initVisibilityHandler() {
  document.addEventListener('visibilitychange', function() {
    if (document.hidden) {
      // Pause: cancel scheduled fetches and animation
      if (fetchDataTimerId !== null) { clearTimeout(fetchDataTimerId); fetchDataTimerId = null; }
      if (fetchQBTimerId !== null) { clearTimeout(fetchQBTimerId); fetchQBTimerId = null; }
      if (animFrameId !== null) { cancelAnimationFrame(animFrameId); animFrameId = null; }
    } else {
      // Resume: restart fetches and animation
      fetchData();
      fetchQuantaBeta();
      if (ctx && canvas && animFrameId === null) {
        animateParticles();
      }
    }
  });
}

// ── Cooldown CSS injection (Agent 8) ────────────────────────
function injectCooldownCss() {
  var style = document.createElement('style');
  style.textContent = '.alert-row.cooldown .alert-dot { background: var(--warn); opacity: 0.5; }' +
    '.alert-row.cooldown .alert-status { color: var(--text-muted); font-style: italic; }';
  document.head.appendChild(style);
}

// ── Bootstrap ─────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', function() {
  // Inject cooldown CSS (Agent 8)
  injectCooldownCss();

  // Tooltip element
  const tooltip = document.createElement('div');
  tooltip.id = 'sparkline-tooltip';
  tooltip.className = 'sparkline-tooltip';
  document.body.appendChild(tooltip);

  // Clock
  updateClock();
  setInterval(updateClock, 1000);

  // Countdown
  countdownTimer = setInterval(tickCountdown, 1000);

  // Particle canvas
  initParticles();

  // Floating glyphs
  initGlyphs();

  // Keyboard shortcuts
  initKeyboardShortcuts();

  // Sort controls
  initSortControls();

  // Tab navigation (Agent 4 — enhanced with arrow keys)
  initNavigation();

  // Governance form (Agent 6)
  initGovernanceForm();

  // Hash-based tab routing (Agent 4 uses pushState, fall back to hash)
  var initialSection = null;
  if (location.hash) {
    initialSection = location.hash.slice(1);
  } else if (history.state && history.state.section) {
    initialSection = history.state.section;
  }
  if (initialSection && document.getElementById('section-' + initialSection)) {
    switchSection(initialSection);
  }

  // Popstate listener (Agent 4)
  window.addEventListener('popstate', function(e) {
    if (e.state && e.state.section) {
      switchSection(e.state.section, true);
    }
  });

  // Visibility handler (Agent 11)
  initVisibilityHandler();

  // Initial data fetch — each self-schedules via setTimeout + backoff (Agent 2)
  fetchData();
  fetchQuantaBeta();

  // History fetch (initial + slow interval)
  fetchHistory();
  setInterval(fetchHistory, HISTORY_INTERVAL_MS);

  // MarketScope scenario generator
  initMarketScope();
});

// ── Section Navigation (Agent 4 — roving tabindex + arrow keys + Agent 10 — aria) ──

function initNavigation() {
  const tabs = document.querySelectorAll('.nav-tab');
  tabs.forEach(function(tab) {
    tab.addEventListener('click', function() {
      switchSection(tab.dataset.section);
    });
  });

  // Arrow key navigation within tablist (Agent 4)
  var tablist = document.getElementById('nav-tabs');
  if (tablist) {
    tablist.addEventListener('keydown', function(e) {
      var tabArr = Array.from(tablist.querySelectorAll('.nav-tab'));
      var idx = tabArr.indexOf(document.activeElement);
      if (idx < 0) return;
      var newIdx = -1;
      if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
        newIdx = (idx + 1) % tabArr.length;
      } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
        newIdx = (idx - 1 + tabArr.length) % tabArr.length;
      } else if (e.key === 'Home') {
        newIdx = 0;
      } else if (e.key === 'End') {
        newIdx = tabArr.length - 1;
      }
      if (newIdx >= 0) {
        e.preventDefault();
        tabArr[newIdx].focus();
        switchSection(tabArr[newIdx].dataset.section);
      }
    });
  }
}

function switchSection(name, fromPopstate) {
  var tabs = document.querySelectorAll('.nav-tab');
  var sections = document.querySelectorAll('.app-section');

  tabs.forEach(function(t) {
    t.classList.remove('active');
    t.setAttribute('aria-selected', 'false');
    t.setAttribute('tabindex', '-1');
  });
  sections.forEach(function(s) {
    s.classList.remove('active');
    s.setAttribute('hidden', '');
  });

  var targetTab = document.querySelector('.nav-tab[data-section="' + name + '"]');
  var targetSection = document.getElementById('section-' + name);

  if (targetTab) {
    targetTab.classList.add('active');
    targetTab.setAttribute('aria-selected', 'true');
    targetTab.setAttribute('tabindex', '0');
  }
  if (targetSection) {
    targetSection.classList.add('active');
    targetSection.removeAttribute('hidden');
  }

  // Use pushState instead of hash (Agent 4), skip if called from popstate
  if (!fromPopstate) {
    history.pushState({ section: name }, '', '#' + name);
  }
}

// ── MarketScope Integration (Agent 5 — hardened) ───────────────────

function initMarketScope() {
  const btn = document.getElementById('ms-generate');
  if (!btn) return;

  btn.addEventListener('click', generateMarketScopeScenarios);

  const valInput = document.getElementById('ms-values');
  if (valInput) {
    valInput.addEventListener('keydown', function(e) {
      if (e.key === 'Enter') generateMarketScopeScenarios();
    });
  }

  // Check MarketScope health on load (Agent 5 — hardened)
  fetch('/api/marketscope/health')
    .then(function(r) {
      if (!r.ok) throw new Error('HTTP ' + r.status);
      return r.json();
    })
    .then(function(data) {
      var badge = document.getElementById('ms-status');
      if (!badge) return;
      if (data.status === 'healthy') {
        badge.textContent = 'ONLINE';
        badge.style.color = '#00ff66';
        badge.style.borderColor = '#00ff66';
        badge.style.background = '#00ff6622';
      } else if (data.status === 'degraded') {
        badge.textContent = 'DEGRADED';
        badge.style.color = '#ffaa00';
        badge.style.borderColor = '#ffaa00';
        badge.style.background = '#ffaa0022';
      }
    })
    .catch(function() {
      var badge = document.getElementById('ms-status');
      if (badge) {
        badge.textContent = 'OFFLINE';
        badge.style.color = '#ff3333';
      }
    });
}

async function generateMarketScopeScenarios() {
  const btn = document.getElementById('ms-generate');
  const symbolEl = document.getElementById('ms-symbol');
  const valuesEl = document.getElementById('ms-values');

  if (!btn || !symbolEl || !valuesEl) return;

  const symbol = symbolEl.value.trim() || 'SPX';
  // Trim and validate each value (Agent 5)
  if (!valuesEl.value.trim()) {
    showMsError('Enter comma-separated numeric values');
    return;
  }

  const values = valuesEl.value.split(',')
    .map(function(v) { return parseFloat(v.trim()); })
    .filter(function(v) { return !isNaN(v) && isFinite(v); });

  if (values.length === 0) {
    showMsError('Enter comma-separated numeric values');
    return;
  }
  if (values.length < 2) {
    showMsError('Need at least 2 numeric values');
    return;
  }

  clearMsResults();

  btn.disabled = true;
  btn.classList.add('loading');
  btn.innerHTML = '<span class="ms-btn-glyph">&#x2B21;</span> GENERATING...';
  var statusEl = document.getElementById('ms-status');
  if (statusEl) statusEl.textContent = 'COMPUTING';

  try {
    const resp = await fetch('/api/scenarios/generate', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ symbol: symbol, values: values }),
    });
    // Parse JSON before checking res.ok for better error messages (Agent 1)
    const data = await resp.json();

    if (!resp.ok || data.error) {
      showMsError(data.error || ('HTTP ' + resp.status));
      return;
    }

    renderMarketScopeResult(data);
  } catch (err) {
    showMsError('Failed to reach scenario engine: ' + err.message);
  } finally {
    btn.disabled = false;
    btn.classList.remove('loading');
    btn.innerHTML = '<span class="ms-btn-glyph">&#x2B21;</span> GENERATE SCENARIO BANDS';
    if (statusEl) statusEl.textContent = 'READY';
  }
}

function renderMarketScopeResult(data) {
  const discEl = document.getElementById('ms-disclaimer');
  const discText = document.getElementById('ms-disclaimer-text');
  if (discEl && discText) {
    discText.textContent = data.disclaimer;
    discEl.style.display = 'flex';
    discEl.classList.remove('ms-error');
  }

  const statsEl = document.getElementById('ms-stats');
  if (statsEl && data.statistics) {
    const s = data.statistics;
    statsEl.innerHTML = [
      { label: 'MEAN', value: safeStat(s.mean) },
      { label: 'STD DEV', value: safeStat(s.std_dev) },
      { label: 'MIN', value: safeStat(s.min) },
      { label: 'MAX', value: safeStat(s.max) },
      { label: 'COUNT', value: s.count || 0 },
    ].map(function(m) {
      return '<div class="ms-stat"><div class="ms-stat-label">' + escapeHtml(m.label) + '</div><div class="ms-stat-value">' + escapeHtml(String(m.value)) + '</div></div>';
    }).join('');
    statsEl.style.display = 'grid';
  }

  const scenEl = document.getElementById('ms-scenarios');
  if (scenEl && data.bands) {
    if (data.bands.length === 0) {
      scenEl.innerHTML = '<div class="empty-state">No scenario bands computed</div>';
    } else {
      const typeClass = { bull_case: 'bull', base_case: 'base', bear_case: 'bear', tail_risk: 'tail' };
      scenEl.innerHTML = data.bands.map(function(sc) {
        return '<div class="ms-scenario ' + escapeHtml(typeClass[sc.scenario] || '') + '">' +
          '<span class="ms-scenario-label">' + escapeHtml(sc.label) + '</span>' +
          '<span class="ms-scenario-range">[' + escapeHtml(sc.range[0].toFixed(2)) + ' \u2014 ' + escapeHtml(sc.range[1].toFixed(2)) + ']</span>' +
          '<span class="ms-scenario-prob">' + escapeHtml(sc.probability) + '</span>' +
        '</div>';
      }).join('');
    }
  }

  const auditEl = document.getElementById('ms-audit');
  if (auditEl && data.data_hash) {
    auditEl.innerHTML =
      '<span>Data Hash: ' + escapeHtml(data.data_hash) + '</span>' +
      '<span>Symbol: ' + escapeHtml(data.symbol) + '</span>' +
      '<span>Samples: ' + escapeHtml(String(data.statistics ? data.statistics.count : '--')) + '</span>';
    auditEl.style.display = 'block';
  }
}

function showMsError(msg) {
  const discEl = document.getElementById('ms-disclaimer');
  const discText = document.getElementById('ms-disclaimer-text');
  if (discEl && discText) {
    discText.textContent = msg;
    discEl.style.display = 'flex';
    discEl.classList.add('ms-error');
  }
}
