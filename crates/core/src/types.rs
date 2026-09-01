use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetClass {
    Crypto,
    Equity,
    Index,
}

impl AssetClass {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "crypto" => Self::Crypto,
            "equity" | "stock" => Self::Equity,
            "index" => Self::Index,
            _ => Self::Equity,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Holding {
    pub symbol: String,
    pub asset_class: AssetClass,
    pub qty: f64,
    pub avg_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    pub symbol: String,
    pub price: f64,
    pub change_pct: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceSnapshot {
    pub prices: HashMap<String, Price>,
    pub timestamp: DateTime<Utc>,
    pub data_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub title: String,
    pub url: String,
    pub source: String,
    pub published: DateTime<Utc>,
    pub relevance_tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub source: String,
    pub ticker: String,
    pub region: String,
    pub timezone: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_class_from_str() {
        assert_eq!(AssetClass::from_str("crypto"), AssetClass::Crypto);
        assert_eq!(AssetClass::from_str("equity"), AssetClass::Equity);
        assert_eq!(AssetClass::from_str("index"), AssetClass::Index);
        assert_eq!(AssetClass::from_str("stock"), AssetClass::Equity);
    }

    #[test]
    fn test_holding_serialize() {
        let h = Holding {
            symbol: "XRP".to_string(),
            asset_class: AssetClass::Crypto,
            qty: 434.841,
            avg_cost: 1.05,
        };
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("XRP"));
        assert!(json.contains("crypto"));
    }
}
