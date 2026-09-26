//! CRYPTEK VIGIL — Structured Market Data Types
//!
//! OHLCV, FundingRate, FuturesBasis types with Ed25519 payload verification.
//! Inspired by SNAPKITTYWEST/market-scope ingestor patterns.

pub mod verify;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OHLCV {
    pub symbol: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRate {
    pub symbol: String,
    pub rate: f64,
    pub next_funding: DateTime<Utc>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesBasis {
    pub symbol: String,
    pub spot_price: f64,
    pub futures_price: f64,
    pub basis_pct: f64,
    pub expiry: DateTime<Utc>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationPair {
    pub asset_a: String,
    pub asset_b: String,
    pub current_corr: f64,
    pub historical_avg: f64,
    pub window_days: u32,
}

impl CorrelationPair {
    pub fn is_breakdown(&self) -> bool {
        self.current_corr < 0.3 && self.historical_avg > 0.7
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MarketData {
    #[serde(rename = "ohlcv")]
    Ohlcv(OHLCV),
    #[serde(rename = "funding_rate")]
    Funding(FundingRate),
    #[serde(rename = "futures_basis")]
    Basis(FuturesBasis),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ohlcv_serde() {
        let ohlcv = OHLCV {
            symbol: "XRP".into(), open: 0.58, high: 0.62,
            low: 0.56, close: 0.60, volume: 1_000_000.0,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&ohlcv).unwrap();
        let parsed: OHLCV = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.symbol, "XRP");
        assert!((parsed.close - 0.60).abs() < 0.001);
    }

    #[test]
    fn test_market_data_enum_tagged() {
        let data = MarketData::Ohlcv(OHLCV {
            symbol: "BTC".into(), open: 50000.0, high: 51000.0,
            low: 49000.0, close: 50500.0, volume: 5000.0,
            timestamp: Utc::now(),
        });
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"type\":\"ohlcv\""));
    }

    #[test]
    fn test_correlation_breakdown() {
        let pair = CorrelationPair {
            asset_a: "SPX".into(), asset_b: "NKX".into(),
            current_corr: 0.25, historical_avg: 0.75, window_days: 90,
        };
        assert!(pair.is_breakdown());

        let normal = CorrelationPair {
            asset_a: "SPX".into(), asset_b: "NDX".into(),
            current_corr: 0.92, historical_avg: 0.95, window_days: 90,
        };
        assert!(!normal.is_breakdown());
    }

    #[test]
    fn test_funding_rate_serde() {
        let fr = FundingRate {
            symbol: "BTC-PERP".into(), rate: 0.0001,
            next_funding: Utc::now(), timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&fr).unwrap();
        let parsed: FundingRate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.symbol, "BTC-PERP");
    }

    #[test]
    fn test_futures_basis() {
        let fb = FuturesBasis {
            symbol: "ETH-0930".into(), spot_price: 1800.0,
            futures_price: 1820.0, basis_pct: 1.11,
            expiry: Utc::now(), timestamp: Utc::now(),
        };
        assert!((fb.basis_pct - 1.11).abs() < 0.01);
    }
}
