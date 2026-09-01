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
        vigil_alerts::get_alert_states(&db)
    };

    let (worm_valid, worm_count) = {
        let wdb = state.worm_db.lock().unwrap();
        vigil_worm::verify(&wdb).unwrap_or((false, 0))
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
            "last_hash": worm_last.first().map(|b| b.hash.clone()).unwrap_or_default(),
            "last_type": worm_last.first().map(|b| b.block_type.clone()).unwrap_or_default(),
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
        vigil_worm::append(
            &wdb,
            "position_sync",
            serde_json::json!({"source": "api", "positions_synced": synced}),
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
        vigil_worm::append(
            &wdb,
            "position_removed",
            serde_json::json!({"symbol": symbol}),
        ).ok();
    }

    axum::Json(serde_json::json!({"deleted": deleted, "symbol": symbol}))
}

async fn scheduler_loop(state: AppState) {
    let alert_config = vigil_alerts::AlertConfig {
        threshold_pct: state.config.hysteresis_pct * 4.0,
        hysteresis_pct: state.config.hysteresis_pct,
        cooldown_minutes: state.config.alert_cooldown_minutes as i64,
    };

    loop {
        let holdings = {
            let db = state.db.lock().unwrap();
            vigil_core::db::get_holdings(&db).unwrap_or_default()
        };

        let snapshot = vigil_ingestor::fetch_all_prices(&holdings, &state.indices, &state.config.finnhub_key).await;

        {
            let wdb = state.worm_db.lock().unwrap();
            vigil_worm::append(&wdb, "price_fetch", serde_json::json!({"hash": &snapshot.data_hash})).ok();
        }

        let fired = {
            let db = state.db.lock().unwrap();
            vigil_alerts::check_alerts(&db, &snapshot, &holdings, &alert_config)
        };

        for alert in &fired {
            vigil_alerts::send_ntfy(&state.config.ntfy_topic, alert).await;
            let wdb = state.worm_db.lock().unwrap();
            vigil_worm::append(&wdb, "alert_fired", serde_json::json!({"symbol": alert.symbol, "direction": alert.direction})).ok();
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
    vigil_worm::seed_genesis(&worm_db)?;

    let positions_path = project_dir.join("positions.txt");
    if positions_path.exists() {
        let holdings = vigil_core::db::get_holdings(&db)?;
        if holdings.is_empty() {
            let count = vigil_core::db::import_positions(&db, &positions_path)?;
            tracing::info!("Imported {count} holdings from positions.txt");
        }
    }

    let indices_path = project_dir.join("packages/connectors/indices.json");
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
    };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/data", get(api_data))
        .route("/api/history", get(api_history))
        .route("/api/sync", post(api_sync))
        .route("/api/positions/:symbol", axum::routing::delete(api_delete_position))
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
