//! CRYPTEK VIGIL — Scenario Band Generator
//!
//! Produces probabilistic scenario bands (NOT predictions).
//! SCENARIO BANDS — NOT PREDICTIONS — Human Review Required

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioBand {
    pub scenario: String,
    pub label: String,
    pub range: (f64, f64),
    pub probability: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioBands {
    pub symbol: String,
    pub statistics: Statistics,
    pub bands: Vec<ScenarioBand>,
    pub data_hash: String,
    pub disclaimer: String,
}

pub fn generate_scenario_bands(prices: &[f64], symbol: &str) -> Option<ScenarioBands> {
    if prices.len() < 2 { return None; }
    let n = prices.len() as f64;
    let mean = prices.iter().sum::<f64>() / n;
    let variance = prices.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std_dev = variance.sqrt();
    let min = prices.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = prices.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let bands = vec![
        ScenarioBand {
            scenario: "bull_case".to_string(),
            label: "Bull Case (75th percentile)".to_string(),
            range: (mean + std_dev, mean + 2.0 * std_dev),
            probability: "low".to_string(),
        },
        ScenarioBand {
            scenario: "base_case".to_string(),
            label: "Base Case (median)".to_string(),
            range: (mean - 0.5 * std_dev, mean + 0.5 * std_dev),
            probability: "medium".to_string(),
        },
        ScenarioBand {
            scenario: "bear_case".to_string(),
            label: "Bear Case (25th percentile)".to_string(),
            range: (mean - 2.0 * std_dev, mean - std_dev),
            probability: "low".to_string(),
        },
        ScenarioBand {
            scenario: "tail_risk".to_string(),
            label: "Tail Risk (5th percentile)".to_string(),
            range: (mean - 3.0 * std_dev, mean - 2.0 * std_dev),
            probability: "very_low".to_string(),
        },
    ];

    let data_hash = {
        let mut hasher = Sha256::new();
        for p in prices { hasher.update(p.to_le_bytes()); }
        format!("{:x}", hasher.finalize())
    };

    Some(ScenarioBands {
        symbol: symbol.to_string(),
        statistics: Statistics { mean, std_dev, min, max, count: prices.len() },
        bands,
        data_hash,
        disclaimer: "SCENARIO BANDS — NOT PREDICTIONS — Human Review Required".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenario_bands_basic() {
        let prices = vec![100.0, 102.0, 98.0, 101.0, 99.0, 103.0, 97.0, 100.0];
        let bands = generate_scenario_bands(&prices, "TEST").unwrap();
        assert_eq!(bands.symbol, "TEST");
        assert_eq!(bands.bands.len(), 4);
        assert_eq!(bands.statistics.count, 8);
        assert!((bands.statistics.mean - 100.0).abs() < 1.0);
        assert!(bands.statistics.std_dev > 0.0);
        assert_eq!(bands.bands[0].scenario, "bull_case");
        assert_eq!(bands.bands[3].scenario, "tail_risk");
        assert!(bands.bands[0].range.0 > bands.statistics.mean);
        assert!(bands.bands[2].range.1 < bands.statistics.mean);
        assert!(!bands.data_hash.is_empty());
    }

    #[test]
    fn test_insufficient_data() {
        assert!(generate_scenario_bands(&[100.0], "X").is_none());
        assert!(generate_scenario_bands(&[], "X").is_none());
    }

    #[test]
    fn test_hash_deterministic() {
        let prices = vec![1.0, 2.0, 3.0];
        let a = generate_scenario_bands(&prices, "A").unwrap();
        let b = generate_scenario_bands(&prices, "A").unwrap();
        assert_eq!(a.data_hash, b.data_hash);
    }
}
