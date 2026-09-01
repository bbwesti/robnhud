/* ============================================================
   CRYPTEK VIGIL — vigil.js
   Dynasty Wealth Monitoring Protocol
   ============================================================ */

'use strict';

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

// ── Formatters ────────────────────────────────────────────────
const fmt = {
  currency: v => {
    const n = Number(v);
    if (isNaN(n)) return '--';
    return '$' + Math.abs(n).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  },
  pct: v => {
    const n = Number(v);
    if (isNaN(n)) return '--';
    return (n >= 0 ? '+' : '') + n.toFixed(2) + '%';
  },
  qty: v => {
    const n = Number(v);
    if (isNaN(n)) return '--';
    return n.toLocaleString('en-US', { minimumFractionDigits: 3, maximumFractionDigits: 6 });
  },
  short: s => {
    if (!s) return '--';
    return s.length > 20 ? s.slice(0, 10) + '…' + s.slice(-6) : s;
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

// ── Sparkline Renderer ────────────────────────────────────────
function drawSparkline(canvasEl, prices, color) {
  color = color || '#00ff66';
  if (!canvasEl || !prices || prices.length < 2) return;
  const dpr = window.devicePixelRatio || 1;
  const w = canvasEl.width;
  const h = canvasEl.height;
  const c = canvasEl.getContext('2d');
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
  const isRgb = color.startsWith('rgb');
  let fillColor;
  if (isRgb) {
    fillColor = color.replace(')', ',0.12)').replace('rgb', 'rgba');
  } else if (color.startsWith('#')) {
    // Convert hex to rgba
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
  const lastX = w;
  const lastY = h - ((prices[prices.length - 1] - min) / range) * (h - 4) - 2;
  c.beginPath();
  prices.forEach(function(p, i) {
    const x = (i / (prices.length - 1)) * w;
    const y = h - ((p - min) / range) * (h - 4) - 2;
    if (i === 0) c.moveTo(x, y);
    else c.lineTo(x, y);
  });
  c.lineTo(lastX, h);
  c.lineTo(0, h);
  c.closePath();
  c.fillStyle = grad;
  c.fill();
}

// Attach tooltip behavior to a sparkline canvas
function attachSparklineTooltip(canvasEl, prices, symbol) {
  const tooltip = document.getElementById('sparkline-tooltip');
  if (!tooltip || !prices || prices.length < 2) return;

  canvasEl.addEventListener('mousemove', function(e) {
    const rect = canvasEl.getBoundingClientRect();
    const relX = e.clientX - rect.left;
    const idx = Math.min(prices.length - 1, Math.max(0, Math.round((relX / rect.width) * (prices.length - 1))));
    const price = prices[idx];
    tooltip.textContent = (symbol ? symbol + ': ' : '') + fmt.currency(price);
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

    // Group by symbol, oldest first
    const grouped = {};
    rows.forEach(function(r) {
      if (!grouped[r.symbol]) grouped[r.symbol] = [];
      grouped[r.symbol].unshift(r.price); // rows come DESC, reverse to ASC
    });
    historyData = grouped;

    // Build portfolio total history: sum values per timestamp bucket
    // Use the latest lastData holdings for qty mapping
    if (lastData && lastData.holdings) {
      const qtyMap = {};
      lastData.holdings.forEach(function(h) { qtyMap[h.symbol] = h.qty; });

      // Find shortest history length across held symbols
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

    // Redraw any existing sparklines in holding rows
    if (lastData && lastData.holdings) {
      redrawHoldingSparklines();
    }
    // Redraw index sparklines
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
  // Color: green if net positive trend, red otherwise
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

// ── Clock ─────────────────────────────────────────────────────
function updateClock() {
  const now = new Date();
  const h   = String(now.getHours()).padStart(2, '0');
  const m   = String(now.getMinutes()).padStart(2, '0');
  const s   = String(now.getSeconds()).padStart(2, '0');

  const elTime = document.getElementById('clock-time');
  if (elTime) elTime.textContent = `${h}:${m}:${s}`;

  // Necron cycle counter — minutes since Unix epoch
  const cycleNum = Math.floor(Date.now() / 60000);
  const elCycle  = document.getElementById('clock-cycle');
  if (elCycle) elCycle.textContent = `CYCLE: ${cycleNum.toString().padStart(8, '0')}`;
}

// ── Countdown Bar ─────────────────────────────────────────────
function resetCountdown() {
  countdownSecs = 60;
  renderCountdown();
}

function renderCountdown() {
  const fill = document.getElementById('countdown-fill');
  if (!fill) return;
  const pct = (countdownSecs / 60) * 100;
  fill.style.width = pct + '%';
}

function tickCountdown() {
  if (countdownSecs > 0) {
    countdownSecs--;
    renderCountdown();
  }
}

// ── Data Fetch ────────────────────────────────────────────────
async function fetchData() {
  try {
    const res  = await fetch('/api/data');
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
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

    resetCountdown();

    // After DOM is updated, redraw sparklines from cached history
    if (Object.keys(historyData).length > 0) {
      redrawHoldingSparklines();
      redrawIndexSparklines();
      drawVaultChart();
    }
  } catch (err) {
    console.error('[VIGIL] fetch error:', err);
    flashError('TRANSMISSION INTERRUPTED — ' + err.message);
  }
}

function flashError(msg) {
  const el = document.getElementById('error-banner');
  if (!el) return;
  el.textContent = msg;
  el.style.display = 'block';
  setTimeout(() => { el.style.display = 'none'; }, 5000);
}

// ── DOM Updaters ──────────────────────────────────────────────
function updatePortfolio(p) {
  if (!p) return;
  const elVal = document.getElementById('vault-value');
  if (elVal) elVal.textContent = fmt.currency(p.total_value);

  const elPl  = document.getElementById('vault-pl');
  if (elPl) {
    const sign = Number(p.total_pl) >= 0 ? '+' : '';
    elPl.textContent = `${sign}${fmt.currency(p.total_pl)} (${fmt.pct(p.total_pl_pct)})`;
    elPl.className   = 'vault-pl ' + signClass(p.total_pl);
  }
}

function sortedHoldings(holdings) {
  const arr = holdings.slice();
  if (holdingsSortKey === 'value') {
    arr.sort(function(a, b) { return Number(b.value) - Number(a.value); });
  } else if (holdingsSortKey === 'symbol') {
    arr.sort(function(a, b) { return a.symbol.localeCompare(b.symbol); });
  } else if (holdingsSortKey === 'change') {
    arr.sort(function(a, b) { return Number(b.change_pct) - Number(a.change_pct); });
  }
  return arr;
}

function updateHoldings(holdings) {
  if (!Array.isArray(holdings)) return;
  const list = document.getElementById('holdings-list');
  if (!list) return;

  list.innerHTML = '';

  const sorted = sortedHoldings(holdings);

  sorted.forEach(function(h) {
    const row = document.createElement('div');
    row.className = 'holding-row';
    row.dataset.symbol = h.symbol;

    const chgClass = dirClass(h.change_pct);
    const plSign   = Number(h.pl) >= 0 ? '+' : '';

    row.innerHTML = `
      <div>
        <div class="holding-symbol">${h.symbol}</div>
        <div class="holding-class">${h.asset_class || ''}</div>
      </div>
      <div>
        <div class="holding-price">${fmt.currency(h.price)}</div>
        <div class="holding-change ${chgClass}">${fmt.pct(h.change_pct)}</div>
      </div>
      <div class="holding-sparkline-wrap">
        <canvas class="sparkline" width="120" height="30" aria-label="${h.symbol} price history"></canvas>
      </div>
      <div class="holding-detail">
        <div class="detail-grid">
          <div>
            <div class="detail-label">QTY</div>
            <div class="detail-val">${fmt.qty(h.qty)}</div>
          </div>
          <div>
            <div class="detail-label">AVG COST</div>
            <div class="detail-val">${fmt.currency(h.avg_cost)}</div>
          </div>
          <div>
            <div class="detail-label">VALUE</div>
            <div class="detail-val">${fmt.currency(h.value)}</div>
          </div>
          <div>
            <div class="detail-label">P/L</div>
            <div class="detail-val ${signClass(h.pl)}">${plSign}${fmt.currency(h.pl)}</div>
          </div>
        </div>
      </div>
    `;

    // Draw sparkline if we have history for this symbol
    const prices = historyData[h.symbol];
    const canvasEl = row.querySelector('canvas.sparkline');
    if (canvasEl && prices && prices.length >= 2) {
      const trend = prices[prices.length - 1] - prices[0];
      drawSparkline(canvasEl, prices, trend >= 0 ? '#00ff66' : '#ff3333');
      attachSparklineTooltip(canvasEl, prices, h.symbol);
    }

    row.addEventListener('click', function(e) {
      // Don't toggle expand if clicking the sparkline canvas
      if (e.target.tagName === 'CANVAS') return;
      const wasExpanded = row.classList.contains('expanded');
      list.querySelectorAll('.holding-row').forEach(function(r) { r.classList.remove('expanded'); });
      if (!wasExpanded) row.classList.add('expanded');
    });

    list.appendChild(row);
  });
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
  badge.className   = `regime-badge ${badgeClass}`;
  if (desc) desc.textContent = regime.description || '';
}

function updateAlerts(alerts) {
  if (!Array.isArray(alerts)) return;
  const list = document.getElementById('alerts-list');
  if (!list) return;

  list.innerHTML = '';

  if (alerts.length === 0) {
    list.innerHTML = '<div class="alert-row inactive"><span class="alert-dot"></span><span class="alert-symbol">NO ALERTS</span></div>';
    return;
  }

  alerts.forEach(a => {
    const statusRaw = (a.status || 'inactive').toLowerCase();
    let rowClass = 'inactive';
    if (statusRaw === 'armed')     rowClass = 'armed';
    if (statusRaw === 'triggered') rowClass = 'triggered';

    const dirText = a.last_direction ? ` — ${a.last_direction.toUpperCase()}` : '';
    const row = document.createElement('div');
    row.className = `alert-row ${rowClass}`;
    row.innerHTML = `
      <div class="alert-dot"></div>
      <span class="alert-symbol">${a.symbol}</span>
      <span class="alert-status">${a.status}${dirText}</span>
    `;
    list.appendChild(row);
  });
}

function updateWorm(worm) {
  if (!worm) return;

  const elBlocks = document.getElementById('worm-blocks');
  const elValid  = document.getElementById('worm-valid');
  const elCount  = document.getElementById('worm-count');
  const elHash   = document.getElementById('worm-hash');
  const elType   = document.getElementById('worm-type');

  if (elValid) {
    elValid.textContent = worm.valid ? 'VERIFIED ◆' : 'BREACH DETECTED';
    elValid.className   = `worm-val ${worm.valid ? 'valid' : 'invalid'}`;
  }
  if (elCount) elCount.textContent = worm.block_count ?? '--';
  if (elHash)  elHash.textContent  = worm.last_hash  ? fmt.short(worm.last_hash) : '--';
  if (elType)  elType.textContent  = (worm.last_type || '--').toUpperCase();

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

  indices.forEach(function(idx) {
    const card = document.createElement('div');
    card.className = 'index-card';
    card.dataset.symbol = idx.symbol;
    const chg = dirClass(idx.change_pct);
    card.innerHTML = `
      <div class="index-symbol">${idx.symbol}</div>
      <div class="index-name">${idx.name || ''}</div>
      <div class="index-price">${fmt.currency(idx.price)}</div>
      <div class="index-change ${chg}">${fmt.pct(idx.change_pct)}</div>
      <div class="index-sparkline-wrap">
        <canvas class="sparkline" width="120" height="24" aria-label="${idx.symbol} history"></canvas>
      </div>
    `;

    // Draw sparkline if history available
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

  scenarios.forEach(sc => {
    const item = document.createElement('div');
    item.className = 'scenario-item';

    const bands = Array.isArray(sc.bands) ? sc.bands : [];

    // Find price range for bar scaling
    const vals = bands.map(b => Number(b.price || b.value || 0)).filter(v => !isNaN(v) && v > 0);
    const maxVal = vals.length ? Math.max(...vals) : 1;

    const bandsHtml = bands.map(b => {
      const rawVal = Number(b.price || b.value || 0);
      const pctBar = maxVal > 0 ? ((rawVal / maxVal) * 100).toFixed(1) : 0;
      const pctDisp = b.probability != null ? (Number(b.probability) * 100).toFixed(1) + '%' : '';
      return `
        <div class="scenario-band">
          <div class="band-label">${b.label || b.scenario || ''}</div>
          <div class="band-bar-track">
            <div class="band-bar-fill" style="width:${pctBar}%"></div>
          </div>
          <div class="band-val">${rawVal > 0 ? fmt.currency(rawVal) : '--'}</div>
          <div class="band-pct">${pctDisp}</div>
        </div>
      `;
    }).join('');

    item.innerHTML = `
      <div class="scenario-header">
        <div class="scenario-symbol">${sc.symbol}</div>
        <div class="scenario-hash">${sc.data_hash ? fmt.short(sc.data_hash) : ''}</div>
      </div>
      ${bandsHtml}
      ${sc.disclaimer ? `<div class="scenario-disclaimer">${sc.disclaimer}</div>` : ''}
    `;

    list.appendChild(item);
  });
}

function updateStatus(status) {
  if (!status) return;
  const elCore    = document.getElementById('status-vigil');
  const elDigest  = document.getElementById('status-digest');
  const elRefresh = document.getElementById('status-refresh');

  if (elCore)    elCore.textContent    = (status.vigil_core || '--').toUpperCase();
  if (elDigest)  elDigest.textContent  = status.next_digest || '--';
  if (elRefresh) {
    const d = status.last_refresh ? new Date(status.last_refresh) : null;
    elRefresh.textContent = d ? d.toLocaleTimeString() : '--';
  }
}

// ── Particle System ───────────────────────────────────────────
function initParticles() {
  canvas = document.getElementById('particle-canvas');
  if (!canvas) return;
  ctx = canvas.getContext('2d');

  resizeCanvas();
  window.addEventListener('resize', resizeCanvas);

  particles = [];
  for (let i = 0; i < PARTICLE_COUNT; i++) {
    particles.push(createParticle());
  }

  animateParticles();
}

function resizeCanvas() {
  if (!canvas) return;
  canvas.width  = window.innerWidth;
  canvas.height = window.innerHeight;
}

function createParticle(x, y) {
  const angle = Math.random() * Math.PI * 2;
  const speed = PARTICLE_SPEED * (0.3 + Math.random() * 0.7);
  return {
    x:     x != null ? x : Math.random() * (canvas ? canvas.width  : 800),
    y:     y != null ? y : Math.random() * (canvas ? canvas.height : 600),
    vx:    Math.cos(angle) * speed,
    vy:    Math.sin(angle) * speed,
    r:     1 + Math.random() * 1.5,
    alpha: 0.2 + Math.random() * 0.5,
    life:  Math.random(),          // phase offset for twinkle
    speed: 0.005 + Math.random() * 0.01,
  };
}

function animateParticles() {
  if (!ctx || !canvas) return;

  ctx.clearRect(0, 0, canvas.width, canvas.height);

  // Update & draw particles
  particles.forEach(p => {
    p.x   += p.vx;
    p.y   += p.vy;
    p.life = (p.life + p.speed) % 1;

    const twinkle = 0.4 + 0.6 * Math.sin(p.life * Math.PI * 2);

    // Wrap around edges
    if (p.x < -5)              p.x = canvas.width  + 5;
    if (p.x > canvas.width  + 5) p.x = -5;
    if (p.y < -5)              p.y = canvas.height + 5;
    if (p.y > canvas.height + 5) p.y = -5;

    // Draw particle
    ctx.beginPath();
    ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
    ctx.fillStyle = `rgba(0,255,102,${p.alpha * twinkle})`;
    ctx.fill();
  });

  // Draw connection lines
  for (let i = 0; i < particles.length; i++) {
    for (let j = i + 1; j < particles.length; j++) {
      const dx   = particles[i].x - particles[j].x;
      const dy   = particles[i].y - particles[j].y;
      const dist = Math.sqrt(dx * dx + dy * dy);

      if (dist < CONNECTION_DIST) {
        const alpha = (1 - dist / CONNECTION_DIST) * 0.12;
        ctx.beginPath();
        ctx.moveTo(particles[i].x, particles[i].y);
        ctx.lineTo(particles[j].x, particles[j].y);
        ctx.strokeStyle = `rgba(0,255,102,${alpha})`;
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
    const delay = -(Math.random() * dur);   // start mid-cycle
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
    // Ignore if typing in an input
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;

    const key = e.key.toUpperCase();

    if (key === 'R') {
      // Force refresh
      fetchData();
      fetchHistory();
      flashError('MANUAL REFRESH INITIATED');
      setTimeout(function() {
        const el = document.getElementById('error-banner');
        if (el) { el.style.display = 'none'; }
      }, 1500);
    }

    if (key === 'H') {
      // Toggle holdings expand/collapse all
      holdingsExpanded = !holdingsExpanded;
      const list = document.getElementById('holdings-list');
      if (!list) return;
      list.querySelectorAll('.holding-row').forEach(function(row) {
        if (holdingsExpanded) row.classList.add('expanded');
        else row.classList.remove('expanded');
      });
    }
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

// ── Bootstrap ─────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', function() {
  // Tooltip element
  const tooltip = document.createElement('div');
  tooltip.id = 'sparkline-tooltip';
  tooltip.className = 'sparkline-tooltip';
  document.body.appendChild(tooltip);

  // Clock — tick every second
  updateClock();
  setInterval(updateClock, 1000);

  // Countdown — tick every second
  countdownTimer = setInterval(tickCountdown, 1000);

  // Particle canvas
  initParticles();

  // Floating glyphs
  initGlyphs();

  // Keyboard shortcuts
  initKeyboardShortcuts();

  // Sort controls
  initSortControls();

  // Initial data fetch then auto-refresh
  fetchData();
  setInterval(fetchData, REFRESH_INTERVAL_MS);

  // History fetch (initial + slow interval)
  fetchHistory();
  setInterval(fetchHistory, HISTORY_INTERVAL_MS);
});
