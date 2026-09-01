//! CRYPTEK VIGIL — Market Regime Detection
//!
//! Deterministic classification. Pure logic. No predictions.
//! Replaces Prolog rules from market-scope.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Regime {
    Bull,
    Bear,
    Transition,
    Unknown,
}

impl std::fmt::Display for Regime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Regime::Bull => write!(f, "BULL"),
            Regime::Bear => write!(f, "BEAR"),
            Regime::Transition => write!(f, "TRANSITION"),
            Regime::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketIndicators {
    pub sma_50: f64,
    pub sma_200: f64,
    pub vix: f64,
    pub breadth: f64,
}

pub fn detect_regime(indicators: &MarketIndicators) -> Regime {
    if indicators.sma_50 > indicators.sma_200 && indicators.vix < 20.0 && indicators.breadth > 0.6 {
        Regime::Bull
    } else if indicators.sma_50 < indicators.sma_200 && indicators.vix > 30.0 {
        Regime::Bear
    } else {
        Regime::Transition
    }
}

pub fn describe_regime(regime: Regime) -> &'static str {
    match regime {
        Regime::Bull => "Bullish: SMA50 > SMA200, VIX < 20, breadth > 0.6",
        Regime::Bear => "Bearish: SMA50 < SMA200, VIX > 30",
        Regime::Transition => "Transition: no clear bull/bear signal",
        Regime::Unknown => "Unknown: insufficient data",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationPair {
    pub asset_a: String,
    pub asset_b: String,
    pub current_corr: f64,
    pub historical_avg: f64,
}

pub fn check_correlation_breakdown(pair: &CorrelationPair) -> bool {
    pair.current_corr < 0.3 && pair.historical_avg > 0.7
}

pub fn calculate_sma(prices: &[f64], period: usize) -> Option<f64> {
    if prices.len() < period { return None; }
    let sum: f64 = prices[prices.len() - period..].iter().sum();
    Some(sum / period as f64)
}

pub fn calculate_breadth(symbols_above_sma: usize, total_symbols: usize) -> f64 {
    if total_symbols == 0 { return 0.0; }
    symbols_above_sma as f64 / total_symbols as f64
}

pub fn pearson_correlation(x: &[f64], y: &[f64]) -> Option<f64> {
    if x.len() != y.len() || x.len() < 2 { return None; }
    let n = x.len() as f64;
    let mean_x: f64 = x.iter().sum::<f64>() / n;
    let mean_y: f64 = y.iter().sum::<f64>() / n;
    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for i in 0..x.len() {
        let dx = x[i] - mean_x;
        let dy = y[i] - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }
    let denom = (var_x * var_y).sqrt();
    if denom == 0.0 { return None; }
    Some(cov / denom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bull_regime() {
        let ind = MarketIndicators { sma_50: 5580.0, sma_200: 5320.0, vix: 16.2, breadth: 0.72 };
        assert_eq!(detect_regime(&ind), Regime::Bull);
    }

    #[test]
    fn test_bear_regime() {
        let ind = MarketIndicators { sma_50: 4800.0, sma_200: 5200.0, vix: 35.0, breadth: 0.3 };
        assert_eq!(detect_regime(&ind), Regime::Bear);
    }

    #[test]
    fn test_transition_regime() {
        let ind = MarketIndicators { sma_50: 5100.0, sma_200: 5000.0, vix: 22.0, breadth: 0.55 };
        assert_eq!(detect_regime(&ind), Regime::Transition);
    }

    #[test]
    fn test_correlation_breakdown() {
        let pair = CorrelationPair { asset_a: "SPX".into(), asset_b: "KOSPI".into(), current_corr: 0.28, historical_avg: 0.60 };
        assert!(!check_correlation_breakdown(&pair));
        let pair2 = CorrelationPair { asset_a: "SPX".into(), asset_b: "NKX".into(), current_corr: 0.25, historical_avg: 0.75 };
        assert!(check_correlation_breakdown(&pair2));
    }

    #[test]
    fn test_sma() {
        let prices = vec![10.0, 11.0, 12.0, 13.0, 14.0];
        assert_eq!(calculate_sma(&prices, 3), Some(13.0));
        assert_eq!(calculate_sma(&prices, 10), None);
    }

    #[test]
    fn test_pearson_correlation() {
        let x = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let y = vec![2.0, 4.0, 6.0, 8.0, 10.0];
        let corr = pearson_correlation(&x, &y).unwrap();
        assert!((corr - 1.0).abs() < 0.0001);
        let z = vec![10.0, 8.0, 6.0, 4.0, 2.0];
        let corr2 = pearson_correlation(&x, &z).unwrap();
        assert!((corr2 + 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_breadth() {
        assert_eq!(calculate_breadth(3, 5), 0.6);
        assert_eq!(calculate_breadth(0, 0), 0.0);
    }
}
