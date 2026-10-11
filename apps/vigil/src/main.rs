//! CRYPTEK VIGIL v2.0 — Dynasty Wealth Monitoring Protocol
//!
//! Main binary: Axum web server + scheduler + Cloudflare tunnel.

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use rust_embed::Embed;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Embed)]
#[folder = "../../dashboard/"]
struct DashboardAssets;

#[derive(Clone)]
struct AppState {
    project_dir: std::path::PathBuf,
    config: Arc<vigil_core::Config>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    worm_db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    indices: Arc<Vec<vigil_core::IndexConfig>>,
    latest_snapshot: Arc<RwLock<Option<vigil_core::PriceSnapshot>>>,
    latest_regime: Arc<RwLock<vigil_regime::Regime>>,
    latest_scenarios: Arc<RwLock<Vec<vigil_quant::ScenarioBands>>>,
    latest_quantabeta: Arc<RwLock<Option<quantabeta_bridge::analysis::QuantaBetaReport>>>,
    signing_key: Arc<ed25519_dalek::SigningKey>,
    governance_seen: Arc<RwLock<std::collections::HashSet<String>>>,
}

async fn serve_index() -> Response {
    match DashboardAssets::get("index.html") {
        Some(content) => Html(String::from_utf8_lossy(content.data.as_ref()).to_string()).into_response(),
        None => (StatusCode::NOT_FOUND, "Dashboard not found").into_response(),
    }
}

async fn serve_asset(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    match DashboardAssets::get(&path) {
        Some(content) => {
            let mime = if path.ends_with(".css") { "text/css" }
                else if path.ends_with(".js") { "application/javascript" }
                else { "application/octet-stream" };
            ([(axum::http::header::CONTENT_TYPE, mime)], content.data.to_vec()).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Asset not found").into_response(),
    }
}

async fn api_data(State(state): State<AppState>) -> axum::Json<serde_json::Value> {
    let snapshot = state.latest_snapshot.read().await;
    let regime = state.latest_regime.read().await;
    let scenarios = state.latest_scenarios.read().await;

    let holdings = {
        let db = state.db.lock().unwrap();
        vigil_core::db::get_holdings(&db).unwrap_or_default()
    };

    let alerts = {
        let db = state.db.lock().unwrap();
        vigil_alerts::get_alert_states(&db, state.config.alert_cooldown_minutes as i64)
    };

    let (worm_valid, worm_count, worm_sigs_valid) = {
        let wdb = state.worm_db.lock().unwrap();
        match vigil_worm::verify_signed(&wdb, &state.signing_key.verifying_key()) {
            Ok(report) => (report.chain_valid, report.block_count, report.all_signatures_valid),
            Err(_) => (false, 0, false),
        }
    };

    let worm_last = {
        let wdb = state.worm_db.lock().unwrap();
        vigil_worm::get_last_n(&wdb, 1).unwrap_or_default()
    };

    let mut total_value = 0.0;
    let mut total_cost = 0.0;
    let mut holdings_json = Vec::new();

    for h in &holdings {
        let price_data = snapshot.as_ref().and_then(|s| s.prices.get(&h.symbol));
        let (price, change_pct) = price_data.map(|p| (p.price, p.change_pct)).unwrap_or((0.0, 0.0));
        let value = price * h.qty;
        let cost = h.avg_cost * h.qty;
        total_value += value;
        total_cost += cost;
        holdings_json.push(serde_json::json!({
            "symbol": h.symbol, "asset_class": h.asset_class, "qty": h.qty,
            "avg_cost": h.avg_cost, "price": price, "change_pct": change_pct,
            "value": value, "pl": value - cost,
        }));
    }

    let total_pl = total_value - total_cost;
    let total_pl_pct = if total_cost > 0.0 { (total_pl / total_cost) * 100.0 } else { 0.0 };

    let mut indices_json = Vec::new();
    if let Some(snap) = snapshot.as_ref() {
        for idx in state.indices.iter() {
            if let Some(p) = snap.prices.get(&idx.symbol) {
                indices_json.push(serde_json::json!({
                    "symbol": idx.symbol, "name": idx.name, "price": p.price, "change_pct": p.change_pct,
                }));
            }
        }
    }

    axum::Json(serde_json::json!({
        "portfolio": {
            "total_value": (total_value * 100.0).round() / 100.0,
            "total_pl": (total_pl * 100.0).round() / 100.0,
            "total_pl_pct": (total_pl_pct * 10.0).round() / 10.0,
        },
        "holdings": holdings_json,
        "indices": indices_json,
        "regime": { "classification": format!("{}", *regime), "description": vigil_regime::describe_regime(*regime) },
        "scenarios": *scenarios,
        "alerts": alerts,
        "worm": {
            "block_count": worm_count, "valid": worm_valid,
            "signatures_valid": worm_sigs_valid,
            "signing_pubkey": vigil_worm::keys::pubkey_to_hex(&state.signing_key.verifying_key()),
            "last_hash": worm_last.first().map(|b| b.hash.clone()).unwrap_or_default(),
            "last_type": worm_last.first().map(|b| b.block_type.clone()).unwrap_or_default(),
            "hash_algo": "blake3",
        },
        "status": {
            "vigil_core": "active", "next_digest": "18:00",
            "last_refresh": snapshot.as_ref().map(|s| s.timestamp.to_rfc3339()).unwrap_or_default(),
        },
    }))
}

async fn api_history(State(state): State<AppState>) -> axum::Json<serde_json::Value> {
    let db = state.db.lock().unwrap();
    let mut stmt = db.prepare(
        "SELECT symbol, price, change_pct, timestamp FROM price_history ORDER BY timestamp DESC LIMIT 500"
    ).unwrap();

    let rows: Vec<serde_json::Value> = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "symbol": row.get::<_, String>(0)?,
            "price": row.get::<_, f64>(1)?,
            "change_pct": row.get::<_, f64>(2)?,
            "timestamp": row.get::<_, String>(3)?,
        }))
    }).unwrap().filter_map(|r| r.ok()).collect();

    axum::Json(serde_json::json!({ "history": rows }))
}

/// POST /api/sync — Accept position updates from external sources (e.g. Robinhood MCP)
async fn api_sync(
    State(state): State<AppState>,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let positions = match payload.get("positions").and_then(|p| p.as_array()) {
        Some(arr) => arr,
        None => return axum::Json(serde_json::json!({"error": "missing positions array", "synced": 0})),
    };

    let db = state.db.lock().unwrap();
    let mut synced = 0;

    for pos in positions {
        let symbol = pos.get("symbol").and_then(|s| s.as_str()).unwrap_or("");
        let asset_class = pos.get("asset_class").and_then(|s| s.as_str()).unwrap_or("equity");
        let qty: f64 = pos.get("qty").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let avg_cost: f64 = pos.get("avg_cost").and_then(|v| v.as_f64()).unwrap_or(0.0);

        if symbol.is_empty() || qty == 0.0 {
            continue;
        }

        db.execute(
            "INSERT OR REPLACE INTO holdings (symbol, asset_class, qty, avg_cost) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![symbol, asset_class, qty, avg_cost],
        ).ok();
        synced += 1;
    }

    // WORM log the sync event
    {
        let wdb = state.worm_db.lock().unwrap();
        vigil_worm::append_signed(
            &wdb,
            "position_sync",
            serde_json::json!({"source": "api", "positions_synced": synced}),
            &state.signing_key,
        ).ok();
    }

    tracing::info!("Synced {} positions via /api/sync", synced);
    axum::Json(serde_json::json!({"synced": synced, "status": "ok"}))
}

/// DELETE /api/positions/:symbol — Remove a position
async fn api_delete_position(
    State(state): State<AppState>,
    axum::extract::Path(symbol): axum::extract::Path<String>,
) -> axum::Json<serde_json::Value> {
    let db = state.db.lock().unwrap();
    let deleted = db.execute("DELETE FROM holdings WHERE symbol = ?1", [&symbol]).unwrap_or(0);

    if deleted > 0 {
        let wdb = state.worm_db.lock().unwrap();
        vigil_worm::append_signed(
            &wdb,
            "position_removed",
            serde_json::json!({"symbol": symbol}),
            &state.signing_key,
        ).ok();
    }

    axum::Json(serde_json::json!({"deleted": deleted, "symbol": symbol}))
}

async fn api_quantabeta(State(state): State<AppState>) -> axum::Json<serde_json::Value> {
    let report = state.latest_quantabeta.read().await;
    match &*report {
        Some(r) => axum::Json(serde_json::to_value(r).unwrap_or_default()),
        None => axum::Json(serde_json::json!({"status": "no_data", "message": "QuantaBeta analysis not yet computed"})),
    }
}

async fn api_governance_verify(
    State(state): State<AppState>,
    axum::Json(seal): axum::Json<vigil_governance::GovernanceSeal>,
) -> axum::Json<serde_json::Value> {
    let seen = state.governance_seen.read().await;
    let result = vigil_governance::verify_seal(&seal, &seen);
    drop(seen);

    // Log to WORM chain
    {
        let wdb = state.worm_db.lock().unwrap();
        vigil_worm::append_signed(
            &wdb,
            "governance_verify",
            serde_json::json!({
                "decision_id": seal.decision_id,
                "seal_valid": result.seal_valid,
                "quorum_met": result.quorum_met,
            }),
            &state.signing_key,
        ).ok();
    }

    // If valid and approved, mark as seen for replay protection
    if result.seal_valid && result.quorum_met && !result.is_replay {
        let mut seen = state.governance_seen.write().await;
        seen.insert(seal.decision_id.clone());
    }

    axum::Json(serde_json::to_value(&result).unwrap_or_default())
}

async fn api_verify_payload(
    axum::Json(payload): axum::Json<vigil_market_types::verify::SignedPayload>,
) -> axum::Json<serde_json::Value> {
    let valid = payload.verify();
    axum::Json(serde_json::json!({
        "valid": valid,
        "data_hash": payload.data_hash,
        "pubkey": payload.pubkey,
    }))
}

// ── MarketScope Integration ──────────────────────────────────────────────────

/// POST /api/scenarios/generate — Generate scenario bands from user-provided values
/// Implements the MarketScope quant-engine logic natively in Rust.
async fn api_generate_scenarios(
    State(state): State<AppState>,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let symbol = payload.get("symbol").and_then(|s| s.as_str()).unwrap_or("UNKNOWN");
    let values: Vec<f64> = payload
        .get("values")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
        .unwrap_or_default();

    if values.is_empty() {
        return axum::Json(serde_json::json!({"error": "No values provided", "scenarios": []}));
    }

    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let std = variance.sqrt();
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    // SHA-256 data hash for WORM logging
    let data_hash = {
        use sha2::Digest;
        let canonical = serde_json::json!({"symbol": symbol, "values": values});
        let mut hasher = sha2::Sha256::new();
        hasher.update(serde_json::to_string(&canonical).unwrap_or_default().as_bytes());
        hex::encode(hasher.finalize())
    };

    let scenarios = serde_json::json!([
        {
            "scenario": "bull_case",
            "label": "Bull Case (75th percentile)",
            "range": [mean + std, mean + 2.0 * std],
            "probability": "low",
            "note": "Scenario band \u{2014} NOT a prediction"
        },
        {
            "scenario": "base_case",
            "label": "Base Case (median)",
            "range": [mean - std * 0.5, mean + std * 0.5],
            "probability": "medium",
            "note": "Scenario band \u{2014} NOT a prediction"
        },
        {
            "scenario": "bear_case",
            "label": "Bear Case (25th percentile)",
            "range": [mean - 2.0 * std, mean - std],
            "probability": "low",
            "note": "Scenario band \u{2014} NOT a prediction"
        },
        {
            "scenario": "tail_risk",
            "label": "Tail Risk (5th percentile)",
            "range": [mean - 3.0 * std, mean - 2.0 * std],
            "probability": "very_low",
            "note": "Scenario band \u{2014} NOT a prediction"
        }
    ]);

    // Log to WORM chain
    {
        let wdb = state.worm_db.lock().unwrap();
        vigil_worm::append_signed(
            &wdb,
            "marketscope_scenario",
            serde_json::json!({
                "symbol": symbol,
                "data_hash": data_hash,
                "scenario_count": 4,
                "source": "marketscope_quant_engine",
            }),
            &state.signing_key,
        )
        .ok();
    }

    let timestamp = chrono::Utc::now().to_rfc3339();

    axum::Json(serde_json::json!({
        "symbol": symbol,
        "timestamp": timestamp,
        "data_hash": data_hash,
        "statistics": {
            "mean": (mean * 100.0).round() / 100.0,
            "std": (std * 100.0).round() / 100.0,
            "min": (min * 100.0).round() / 100.0,
            "max": (max * 100.0).round() / 100.0,
            "count": values.len(),
        },
        "scenarios": scenarios,
        "disclaimer": "SCENARIO BANDS \u{2014} NOT PREDICTIONS \u{2014} Human Review Required",
        "training_gate": "Human_Review_Required",
        "audit_spec": "4b565498-9afc-4782-af4a-c6b11a5d0058",
    }))
}

/// POST /api/compliance/check — Check signal compliance against sovereign constraints
async fn api_compliance_check(
    axum::Json(signal): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    let signal_type = signal.get("type").and_then(|s| s.as_str()).unwrap_or("");
    let prohibited = ["buy_signal", "sell_signal", "price_target", "trade_recommendation"];

    if prohibited.contains(&signal_type) {
        return axum::Json(serde_json::json!({
            "compliant": false,
            "violation": "financial_advice_prohibited",
            "message": "This signal type is PROHIBITED under sovereign axioms",
            "audit_spec": "4b565498-9afc-4782-af4a-c6b11a5d0058",
        }));
    }

    axum::Json(serde_json::json!({
        "compliant": true,
        "message": "Signal passed compliance check",
        "training_gate": "Human_Review_Required",
    }))
}

/// GET /api/marketscope/health — MarketScope integration health check
async fn api_marketscope_health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "healthy",
        "service": "marketscope-quant-engine",
        "version": "0.1.0",
        "output_type": "scenario_bands",
        "training_gate": "Human_Review_Required",
        "integrated": true,
        "prolog_rules": ["compliance", "correlation", "regime"],
    }))
}

async fn scheduler_loop(state: AppState) {
    let alert_config = vigil_alerts::AlertConfig {
        threshold_pct: state.config.hysteresis_pct * 4.0,
        hysteresis_pct: state.config.hysteresis_pct,
        cooldown_minutes: state.config.alert_cooldown_minutes as i64,
    };

    let mut cycle_count: u64 = 0;

    loop {
        cycle_count += 1;

        let holdings = {
            let db = state.db.lock().unwrap();
            vigil_core::db::get_holdings(&db).unwrap_or_default()
        };

        let snapshot = vigil_ingestor::fetch_all_prices(&holdings, &state.indices, &state.config.finnhub_key).await;

        {
            let wdb = state.worm_db.lock().unwrap();
            vigil_worm::append_signed(&wdb, "price_fetch", serde_json::json!({"hash": &snapshot.data_hash}), &state.signing_key).ok();
        }

        let fired = {
            let db = state.db.lock().unwrap();
            vigil_alerts::check_alerts(&db, &snapshot, &holdings, &alert_config)
        };

        for alert in &fired {
            vigil_alerts::send_ntfy(&state.config.ntfy_topic, alert).await;
            let wdb = state.worm_db.lock().unwrap();
            vigil_worm::append_signed(&wdb, "alert_fired", serde_json::json!({"symbol": alert.symbol, "direction": alert.direction}), &state.signing_key).ok();
        }

        // Store price history for charts
        {
            let db = state.db.lock().unwrap();
            for (symbol, price) in &snapshot.prices {
                db.execute(
                    "INSERT INTO price_history (symbol, price, change_pct, timestamp) VALUES (?1, ?2, ?3, datetime('now'))",
                    rusqlite::params![symbol, price.price, price.change_pct],
                ).ok();
            }
        }

        // Market regime detection
        {
            let vix = vigil_ingestor::fetch_vix(&state.config.finnhub_key).await;

            // Use SPX (S&P 500) price history for SMA calculations
            let spx_prices: Vec<f64> = {
                let db = state.db.lock().unwrap();
                db.prepare("SELECT price FROM price_history WHERE symbol = 'SPX' ORDER BY timestamp ASC")
                    .and_then(|mut stmt| {
                        let rows = stmt.query_map([], |row| row.get::<_, f64>(0))?;
                        Ok(rows.filter_map(|r| r.ok()).collect())
                    })
                    .unwrap_or_default()
            };

            let sma_50 = vigil_regime::calculate_sma(&spx_prices, 50);
            let sma_200 = vigil_regime::calculate_sma(&spx_prices, 200);

            // Calculate breadth: fraction of holdings whose current price > their own SMA-50
            let breadth = {
                let db = state.db.lock().unwrap();
                let mut above = 0usize;
                let total = holdings.len();
                for h in &holdings {
                    let h_prices: Vec<f64> = db.prepare(
                        "SELECT price FROM price_history WHERE symbol = ?1 ORDER BY timestamp ASC"
                    )
                        .and_then(|mut stmt| {
                            let rows = stmt.query_map(rusqlite::params![h.symbol], |row| row.get::<_, f64>(0))?;
                            Ok(rows.filter_map(|r| r.ok()).collect())
                        })
                        .unwrap_or_default();
                    if let Some(h_sma) = vigil_regime::calculate_sma(&h_prices, 50) {
                        let current = snapshot.prices.get(&h.symbol).map(|p| p.price).unwrap_or(0.0);
                        if current > h_sma {
                            above += 1;
                        }
                    }
                }
                vigil_regime::calculate_breadth(above, total)
            };

            let regime = match (sma_50, sma_200) {
                (Some(s50), Some(s200)) => {
                    let indicators = vigil_regime::MarketIndicators {
                        sma_50: s50, sma_200: s200, vix, breadth,
                    };
                    vigil_regime::detect_regime(&indicators)
                }
                (Some(s50), None) => {
                    // Not enough data for SMA-200 yet — use SMA-50 trend + VIX
                    let latest_spx = spx_prices.last().copied().unwrap_or(s50);
                    if latest_spx > s50 && vix < 20.0 && breadth > 0.6 {
                        vigil_regime::Regime::Bull
                    } else if latest_spx < s50 && vix > 30.0 {
                        vigil_regime::Regime::Bear
                    } else {
                        vigil_regime::Regime::Transition
                    }
                }
                _ => {
                    // No index data yet — VIX-only heuristic
                    if vix > 30.0 {
                        vigil_regime::Regime::Bear
                    } else if vix < 18.0 && breadth > 0.6 {
                        vigil_regime::Regime::Bull
                    } else {
                        vigil_regime::Regime::Transition
                    }
                }
            };
            *state.latest_regime.write().await = regime;
        }

        // Scenario band generation
        {
            let scenarios = {
                let db = state.db.lock().unwrap();
                let mut s = Vec::new();
                for h in &holdings {
                    let prices: Vec<f64> = db.prepare(
                        "SELECT price FROM price_history WHERE symbol = ?1 ORDER BY timestamp ASC"
                    )
                        .and_then(|mut stmt| {
                            let rows = stmt.query_map(rusqlite::params![h.symbol], |row| row.get::<_, f64>(0))?;
                            Ok(rows.filter_map(|r| r.ok()).collect())
                        })
                        .unwrap_or_default();
                    if let Some(bands) = vigil_quant::generate_scenario_bands(&prices, &h.symbol) {
                        s.push(bands);
                    }
                }
                s
            };
            *state.latest_scenarios.write().await = scenarios;
        }

        // QuantaBeta analysis — run every 10 cycles to avoid overhead
        if cycle_count % 10 == 1 {
            let portfolio: Vec<(String, Vec<f64>)> = {
                let db = state.db.lock().unwrap();
                holdings.iter().filter_map(|h| {
                    let mut stmt = db.prepare(
                        "SELECT price FROM price_history WHERE symbol = ?1 ORDER BY timestamp ASC LIMIT 500"
                    ).ok()?;
                    let prices: Vec<f64> = stmt.query_map(rusqlite::params![h.symbol], |row| {
                        row.get::<_, f64>(0)
                    }).ok()?.filter_map(|r| r.ok()).collect();
                    if prices.len() >= 22 {
                        Some((h.symbol.clone(), prices))
                    } else {
                        None
                    }
                }).collect()
            };

            if !portfolio.is_empty() {
                let report = quantabeta_bridge::analysis::build_report(&portfolio, 20);
                tracing::info!(
                    "QuantaBeta analysis complete: {} assets, {} cross-correlations",
                    report.assets.len(),
                    report.cross_correlations.len()
                );

                // Log to WORM chain
                {
                    let wdb = state.worm_db.lock().unwrap();
                    vigil_worm::append_signed(
                        &wdb,
                        "quantabeta_analysis",
                        serde_json::json!({
                            "type": "quantabeta_analysis",
                            "assets_analyzed": report.assets.len(),
                            "cross_correlations": report.cross_correlations.len(),
                            "report_hash": report.report_hash,
                        }),
                        &state.signing_key,
                    ).ok();
                }

                // Cache report in DB
                {
                    let db = state.db.lock().unwrap();
                    quantabeta_bridge::cache::store_report(&db, &report, &report.report_hash).ok();
                }

                *state.latest_quantabeta.write().await = Some(report);
            }
        }

        *state.latest_snapshot.write().await = Some(snapshot);
        tracing::debug!("Price cycle complete. {} alerts fired.", fired.len());
        tokio::time::sleep(std::time::Duration::from_secs(state.config.poll_seconds)).await;
    }
}

async fn tunnel_loop(project_dir: std::path::PathBuf, ntfy_topic: String) {
    let cloudflared = project_dir.join("cloudflared.exe");
    if !cloudflared.exists() {
        tracing::warn!("cloudflared.exe not found — skipping tunnel");
        return;
    }
    let mut last_url: Option<String> = None;
    loop {
        tracing::info!("Starting Cloudflare tunnel...");
        let tunnel_log = project_dir.join("tunnel.log");
        let child = tokio::process::Command::new(&cloudflared)
            .args(["tunnel", "--url", "http://localhost:5555"])
            .stdout(std::fs::File::create(&tunnel_log).unwrap())
            .stderr(std::process::Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => { tracing::error!("Failed to start tunnel: {e}"); tokio::time::sleep(std::time::Duration::from_secs(30)).await; continue; }
        };
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        if let Ok(log_content) = std::fs::read_to_string(&tunnel_log) {
            for line in log_content.lines() {
                if line.contains("trycloudflare.com") && line.contains("https://") {
                    if let Some(start) = line.find("https://") {
                        if let Some(end_offset) = line[start..].find(".trycloudflare.com") {
                            let url = &line[start..start + end_offset + ".trycloudflare.com".len()];
                            tracing::info!("Tunnel URL: {url}");
                            if last_url.as_deref() != Some(url) {
                                if !ntfy_topic.is_empty() {
                                    let _ = reqwest::Client::new()
                                        .post(format!("https://ntfy.sh/{ntfy_topic}"))
                                        .header("Title", "VIGIL — new phone URL")
                                        .header("Tags", "link")
                                        .header("Click", url)
                                        .body(url.to_string())
                                        .send().await;
                                }
                                last_url = Some(url.to_string());
                            }
                        }
                    }
                }
            }
        }
        let _ = child.wait().await;
        tracing::warn!("Tunnel exited — restarting in 10s...");
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let project_dir = std::env::current_dir()?;

    tracing::info!("═══════════════════════════════════════════════════");
    tracing::info!("  CRYPTEK VIGIL v2.0 — Dynasty Wealth Monitoring");
    tracing::info!("═══════════════════════════════════════════════════");

    let config = vigil_core::Config::load(&project_dir)?;
    tracing::info!("Config loaded from {}", project_dir.display());

    let db = vigil_core::db::init_db(&config.db_path)?;
    let worm_db = rusqlite::Connection::open(&config.worm_db_path)?;
    vigil_worm::init_worm_db(&worm_db)?;
    vigil_worm::migrate_signed_columns(&worm_db)?;
    vigil_worm::seed_genesis(&worm_db)?;

    let key_dir = project_dir.join(".vigil_keys");
    std::fs::create_dir_all(&key_dir)?;
    let signing_key = vigil_worm::keys::load_or_generate_signing_key(&key_dir)?;
    tracing::info!("WORM signing key: {}", vigil_worm::keys::pubkey_to_hex(&signing_key.verifying_key()));

    quantabeta_bridge::cache::init_cache_table(&db)?;

    let positions_path = project_dir.join("positions.txt");
    if positions_path.exists() {
        let holdings = vigil_core::db::get_holdings(&db)?;
        if holdings.is_empty() {
            let count = vigil_core::db::import_positions(&db, &positions_path)?;
            tracing::info!("Imported {count} holdings from positions.txt");
        }
    }

    let indices_path = project_dir.join("indices.json");
    let indices = vigil_ingestor::load_index_configs(&indices_path);
    tracing::info!("Tracking {} indices", indices.len());

    let holdings = vigil_core::db::get_holdings(&db)?;
    tracing::info!("Holdings: {}", holdings.iter().map(|h| h.symbol.as_str()).collect::<Vec<_>>().join(", "));

    let state = AppState {
        project_dir: project_dir.clone(),
        config: Arc::new(config.clone()),
        db: Arc::new(std::sync::Mutex::new(db)),
        worm_db: Arc::new(std::sync::Mutex::new(worm_db)),
        indices: Arc::new(indices),
        latest_snapshot: Arc::new(RwLock::new(None)),
        latest_regime: Arc::new(RwLock::new(vigil_regime::Regime::Unknown)),
        latest_scenarios: Arc::new(RwLock::new(Vec::new())),
        latest_quantabeta: Arc::new(RwLock::new(None)),
        signing_key: Arc::new(signing_key),
        governance_seen: Arc::new(RwLock::new(std::collections::HashSet::new())),
    };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/data", get(api_data))
        .route("/api/history", get(api_history))
        .route("/api/sync", post(api_sync))
        .route("/api/quantabeta", get(api_quantabeta))
        .route("/api/positions/:symbol", axum::routing::delete(api_delete_position))
        .route("/api/governance/verify", post(api_governance_verify))
        .route("/api/market-types/verify", post(api_verify_payload))
        // MarketScope integration routes
        .route("/api/scenarios/generate", post(api_generate_scenarios))
        .route("/api/compliance/check", post(api_compliance_check))
        .route("/api/marketscope/health", get(api_marketscope_health))
        .route("/assets/*path", get(serve_asset))
        .with_state(state.clone());

    let sched_state = state.clone();
    tokio::spawn(async move { scheduler_loop(sched_state).await; });

    let tunnel_dir = project_dir.clone();
    let tunnel_topic = config.ntfy_topic.clone();
    tokio::spawn(async move { tunnel_loop(tunnel_dir, tunnel_topic).await; });

    let addr = "0.0.0.0:5555";
    tracing::info!("Dashboard: http://localhost:5555");
    tracing::info!("Alerts: every {}s | Digest: 18:00 | Recalib: 09:00", config.poll_seconds);
    tracing::info!("═══════════════════════════════════════════════════");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
