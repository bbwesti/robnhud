//! CRYPTEK VIGIL — Sovereign Data Ingestor
//!
//! Fetches live prices from CoinGecko (crypto), Finnhub (stocks/VIX), Yahoo Finance (indices).

use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use vigil_core::{AssetClass, Holding, IndexConfig, Price, PriceSnapshot};

fn coingecko_id(symbol: &str) -> Option<&'static str> {
    match symbol.to_uppercase().as_str() {
        "BTC" => Some("bitcoin"),
        "ETH" => Some("ethereum"),
        "XRP" => Some("ripple"),
        "DOGE" => Some("dogecoin"),
        "SOL" => Some("solana"),
        "ADA" => Some("cardano"),
        "DOT" => Some("polkadot"),
        "MATIC" => Some("matic-network"),
        "LINK" => Some("chainlink"),
        "AVAX" => Some("avalanche-2"),
        "SHIB" => Some("shiba-inu"),
        "LTC" => Some("litecoin"),
        "BNB" => Some("binancecoin"),
        _ => None,
    }
}

pub async fn fetch_crypto_prices(symbols: &[String]) -> HashMap<String, Price> {
    let mut id_to_symbol: HashMap<String, String> = HashMap::new();
    let mut ids = Vec::new();
    for sym in symbols {
        let upper = sym.to_uppercase().replace("-USD", "");
        if let Some(id) = coingecko_id(&upper) {
            ids.push(id.to_string());
            id_to_symbol.insert(id.to_string(), upper);
        }
    }
    if ids.is_empty() { return HashMap::new(); }
    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd&include_24hr_change=true",
        ids.join(",")
    );
    let client = reqwest::Client::new();
    let resp = match client.get(&url).timeout(std::time::Duration::from_secs(10)).send().await {
        Ok(r) => r,
        Err(e) => { tracing::warn!("CoinGecko fetch failed: {e}"); return HashMap::new(); }
    };
    let data: serde_json::Value = match resp.json().await {
        Ok(d) => d,
        Err(e) => { tracing::warn!("CoinGecko parse failed: {e}"); return HashMap::new(); }
    };
    let mut prices = HashMap::new();
    if let Some(obj) = data.as_object() {
        for (cg_id, info) in obj {
            if let Some(symbol) = id_to_symbol.get(cg_id) {
                let price = info.get("usd").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let change = info.get("usd_24h_change").and_then(|v| v.as_f64()).unwrap_or(0.0);
                prices.insert(symbol.clone(), Price {
                    symbol: symbol.clone(), price, change_pct: (change * 100.0).round() / 100.0, timestamp: Utc::now(),
                });
            }
        }
    }
    prices
}

pub async fn fetch_finnhub_quote(symbol: &str, api_key: &str) -> Option<Price> {
    if api_key.is_empty() { return None; }
    let url = format!("https://finnhub.io/api/v1/quote?symbol={symbol}&token={api_key}");
    let client = reqwest::Client::new();
    let resp = client.get(&url).timeout(std::time::Duration::from_secs(10)).send().await.ok()?;
    let data: serde_json::Value = resp.json().await.ok()?;
    let current = data.get("c").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if current == 0.0 { return None; }
    let change_pct = data.get("dp").and_then(|v| v.as_f64()).unwrap_or(0.0);
    Some(Price { symbol: symbol.to_string(), price: current, change_pct: (change_pct * 100.0).round() / 100.0, timestamp: Utc::now() })
}

pub async fn fetch_vix(api_key: &str) -> f64 {
    if let Some(price) = fetch_finnhub_quote("VIX", api_key).await { return price.price; }
    if let Some(price) = fetch_finnhub_quote("VIXY", api_key).await { return price.price; }
    25.0
}

pub async fn fetch_index_prices(indices: &[IndexConfig], api_key: &str) -> HashMap<String, Price> {
    let mut prices = HashMap::new();
    for idx in indices {
        let finnhub_symbol = match idx.symbol.as_str() {
            "SPX" => "SPY", "NDX" => "QQQ", "NKX" => "EWJ", "DAX" => "EWG", "KOSPI" => "EWY",
            other => other,
        };
        if let Some(price) = fetch_finnhub_quote(finnhub_symbol, api_key).await {
            prices.insert(idx.symbol.clone(), Price {
                symbol: idx.symbol.clone(), price: price.price, change_pct: price.change_pct, timestamp: Utc::now(),
            });
        }
    }
    prices
}

pub async fn fetch_all_prices(holdings: &[Holding], indices: &[IndexConfig], finnhub_key: &str) -> PriceSnapshot {
    let crypto_symbols: Vec<String> = holdings.iter().filter(|h| h.asset_class == AssetClass::Crypto).map(|h| h.symbol.clone()).collect();
    let stock_symbols: Vec<String> = holdings.iter().filter(|h| h.asset_class == AssetClass::Equity).map(|h| h.symbol.clone()).collect();
    let (crypto_prices, index_prices) = tokio::join!(
        fetch_crypto_prices(&crypto_symbols),
        fetch_index_prices(indices, finnhub_key),
    );
    let mut all_prices = crypto_prices;
    all_prices.extend(index_prices);
    for sym in &stock_symbols {
        if let Some(price) = fetch_finnhub_quote(sym, finnhub_key).await {
            all_prices.insert(sym.clone(), price);
        }
    }
    let data_hash = {
        let mut hasher = Sha256::new();
        let serialized = serde_json::to_vec(&all_prices).unwrap_or_default();
        hasher.update(&serialized);
        format!("{:x}", hasher.finalize())
    };
    PriceSnapshot { prices: all_prices, timestamp: Utc::now(), data_hash }
}

pub fn load_index_configs(path: &std::path::Path) -> Vec<IndexConfig> {
    let content = match std::fs::read_to_string(path) { Ok(c) => c, Err(_) => return Vec::new() };
    let json: serde_json::Value = match serde_json::from_str(&content) { Ok(j) => j, Err(_) => return Vec::new() };
    let connectors = json.get("connectors").and_then(|c| c.as_array());
    match connectors {
        Some(arr) => arr.iter().filter_map(|c| {
            Some(IndexConfig {
                id: c.get("id")?.as_str()?.to_string(),
                symbol: c.get("symbol")?.as_str()?.to_string(),
                name: c.get("name")?.as_str()?.to_string(),
                source: c.get("source")?.as_str()?.to_string(),
                ticker: c.get("ticker")?.as_str()?.to_string(),
                region: c.get("region")?.as_str()?.to_string(),
                timezone: c.get("timezone")?.as_str()?.to_string(),
            })
        }).collect(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coingecko_id_mapping() {
        assert_eq!(coingecko_id("XRP"), Some("ripple"));
        assert_eq!(coingecko_id("DOGE"), Some("dogecoin"));
        assert_eq!(coingecko_id("BTC"), Some("bitcoin"));
        assert_eq!(coingecko_id("UNKNOWN"), None);
    }

    #[test]
    fn test_load_index_configs() {
        let tmp = std::env::temp_dir().join("test_indices.json");
        std::fs::write(&tmp, r#"{"connectors":[{"id":"spx","symbol":"SPX","name":"S&P 500","source":"yahoo","ticker":"^GSPC","region":"US","timezone":"America/New_York"}]}"#).unwrap();
        let configs = load_index_configs(&tmp);
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].symbol, "SPX");
        std::fs::remove_file(tmp).ok();
    }

    #[test]
    fn test_load_missing_file() {
        let configs = load_index_configs(std::path::Path::new("/nonexistent.json"));
        assert!(configs.is_empty());
    }
}
