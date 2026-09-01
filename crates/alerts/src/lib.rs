//! CRYPTEK VIGIL — Gauss Interceptor (Alert Engine)
//!
//! Price alerts with hysteresis, cooldown, and ntfy push notifications.

use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use vigil_core::{Holding, PriceSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiredAlert {
    pub symbol: String,
    pub direction: String,
    pub price: f64,
    pub threshold_pct: f64,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AlertConfig {
    pub threshold_pct: f64,
    pub hysteresis_pct: f64,
    pub cooldown_minutes: i64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self { threshold_pct: 3.0, hysteresis_pct: 0.75, cooldown_minutes: 45 }
    }
}

pub fn check_alerts(conn: &Connection, snapshot: &PriceSnapshot, holdings: &[Holding], config: &AlertConfig) -> Vec<FiredAlert> {
    let mut fired = Vec::new();
    for holding in holdings {
        let price = match snapshot.prices.get(&holding.symbol) { Some(p) => p, None => continue };
        let state: Option<(f64, Option<String>, Option<String>)> = conn
            .query_row("SELECT baseline, last_fired, last_direction FROM alert_state WHERE symbol = ?1", [&holding.symbol], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .ok();
        let baseline = match state {
            Some((b, _, _)) => b,
            None => {
                conn.execute("INSERT OR REPLACE INTO alert_state (symbol, baseline) VALUES (?1, ?2)", rusqlite::params![holding.symbol, price.price]).ok();
                continue;
            }
        };
        let (_, last_fired_str, last_direction) = state.unwrap();
        if let Some(lf) = &last_fired_str {
            if let Ok(last_fired) = DateTime::parse_from_rfc3339(lf) {
                let elapsed = Utc::now() - last_fired.with_timezone(&Utc);
                if elapsed < Duration::minutes(config.cooldown_minutes) { continue; }
            }
        }
        let change_pct = ((price.price - baseline) / baseline) * 100.0;
        if change_pct >= config.threshold_pct {
            let is_repeat = last_direction.as_deref() == Some("up");
            let effective = if is_repeat { config.threshold_pct + config.hysteresis_pct } else { config.threshold_pct };
            if change_pct >= effective {
                let alert = FiredAlert {
                    symbol: holding.symbol.clone(), direction: "up".to_string(), price: price.price, threshold_pct: change_pct,
                    message: format!("{} +{:.1}% — ${:.4} (from ${:.4})", holding.symbol, change_pct, price.price, baseline),
                    timestamp: Utc::now(),
                };
                conn.execute("UPDATE alert_state SET last_fired = ?1, last_direction = 'up' WHERE symbol = ?2", rusqlite::params![Utc::now().to_rfc3339(), holding.symbol]).ok();
                fired.push(alert);
            }
        }
        if change_pct <= -config.threshold_pct {
            let is_repeat = last_direction.as_deref() == Some("down");
            let effective = if is_repeat { config.threshold_pct + config.hysteresis_pct } else { config.threshold_pct };
            if change_pct.abs() >= effective {
                let alert = FiredAlert {
                    symbol: holding.symbol.clone(), direction: "down".to_string(), price: price.price, threshold_pct: change_pct,
                    message: format!("{} {:.1}% — ${:.4} (from ${:.4})", holding.symbol, change_pct, price.price, baseline),
                    timestamp: Utc::now(),
                };
                conn.execute("UPDATE alert_state SET last_fired = ?1, last_direction = 'down' WHERE symbol = ?2", rusqlite::params![Utc::now().to_rfc3339(), holding.symbol]).ok();
                fired.push(alert);
            }
        }
    }
    fired
}

pub fn recalibrate(conn: &Connection, snapshot: &PriceSnapshot) {
    for (symbol, price) in &snapshot.prices {
        conn.execute("INSERT OR REPLACE INTO alert_state (symbol, baseline) VALUES (?1, ?2)", rusqlite::params![symbol, price.price]).ok();
    }
    tracing::info!("Recalibrated {} alert baselines", snapshot.prices.len());
}

pub async fn send_ntfy(topic: &str, alert: &FiredAlert) {
    if topic.is_empty() { return; }
    let client = reqwest::Client::new();
    let title = format!("VIGIL — {} {}", alert.symbol, alert.direction.to_uppercase());
    let _ = client.post(format!("https://ntfy.sh/{topic}"))
        .header("Title", title)
        .header("Tags", if alert.direction == "up" { "chart_with_upwards_trend" } else { "chart_with_downwards_trend" })
        .body(alert.message.clone())
        .timeout(std::time::Duration::from_secs(10))
        .send().await;
}

pub fn get_alert_states(conn: &Connection) -> Vec<serde_json::Value> {
    let mut stmt = conn.prepare("SELECT symbol, baseline, last_fired, last_direction FROM alert_state").unwrap();
    stmt.query_map([], |row| {
        let symbol: String = row.get(0)?;
        let baseline: f64 = row.get(1)?;
        let last_fired: Option<String> = row.get(2)?;
        let last_direction: Option<String> = row.get(3)?;
        Ok(serde_json::json!({
            "symbol": symbol, "baseline": baseline, "last_fired": last_fired, "last_direction": last_direction,
            "status": if last_fired.is_some() { "FIRED" } else { "ARMED" },
        }))
    }).unwrap().filter_map(|r| r.ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vigil_core::{db::init_db, AssetClass, Price};
    use std::path::Path;

    fn setup() -> (Connection, Vec<Holding>) {
        let conn = init_db(Path::new(":memory:")).unwrap();
        let holdings = vec![Holding { symbol: "XRP".to_string(), asset_class: AssetClass::Crypto, qty: 434.841, avg_cost: 1.05 }];
        (conn, holdings)
    }

    fn make_snapshot(symbol: &str, price: f64) -> PriceSnapshot {
        let mut prices = std::collections::HashMap::new();
        prices.insert(symbol.to_string(), Price { symbol: symbol.to_string(), price, change_pct: 0.0, timestamp: Utc::now() });
        PriceSnapshot { prices, timestamp: Utc::now(), data_hash: "test".to_string() }
    }

    #[test]
    fn test_first_check_sets_baseline() {
        let (conn, holdings) = setup();
        let config = AlertConfig::default();
        let snap = make_snapshot("XRP", 0.58);
        let alerts = check_alerts(&conn, &snap, &holdings, &config);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_alert_fires_on_threshold() {
        let (conn, holdings) = setup();
        let config = AlertConfig::default();
        let snap1 = make_snapshot("XRP", 1.00);
        check_alerts(&conn, &snap1, &holdings, &config);
        let snap2 = make_snapshot("XRP", 1.04);
        let alerts = check_alerts(&conn, &snap2, &holdings, &config);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].direction, "up");
    }

    #[test]
    fn test_no_alert_below_threshold() {
        let (conn, holdings) = setup();
        let config = AlertConfig::default();
        let snap1 = make_snapshot("XRP", 1.00);
        check_alerts(&conn, &snap1, &holdings, &config);
        let snap2 = make_snapshot("XRP", 1.02);
        let alerts = check_alerts(&conn, &snap2, &holdings, &config);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_recalibrate() {
        let (conn, _) = setup();
        let snap = make_snapshot("XRP", 0.60);
        recalibrate(&conn, &snap);
        let baseline: f64 = conn.query_row("SELECT baseline FROM alert_state WHERE symbol = 'XRP'", [], |r| r.get(0)).unwrap();
        assert!((baseline - 0.60).abs() < 0.001);
    }
}
