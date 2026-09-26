use rug::Rational;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::convert::rational_to_f64;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AssetAnalysis {
    pub symbol: String,
    pub partition_volatility: Vec<f64>,
    pub entropy_point: f64,
    pub entropy_interval: (f64, f64),
    pub complexity_bound: String,
    pub ramanujan_congruence_holds: bool,
    pub data_points: usize,
    pub audit_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CrossCorrelation {
    pub asset_a: String,
    pub asset_b: String,
    pub hecke_correlation: f64,
    pub level: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuantaBetaReport {
    pub assets: Vec<AssetAnalysis>,
    pub cross_correlations: Vec<CrossCorrelation>,
    pub timestamp: String,
    pub report_hash: String,
}

/// Compute log-returns as rationals: r_i = (p_{i+1} - p_i) / p_i
fn compute_returns(prices: &[Rational]) -> Vec<Rational> {
    if prices.len() < 2 {
        return vec![];
    }
    prices
        .windows(2)
        .filter_map(|w| {
            if w[0] == Rational::from((0i64, 1i64)) {
                None
            } else {
                let diff = Rational::from(&w[1] - &w[0]);
                Some(Rational::from(&diff / &w[0]))
            }
        })
        .collect()
}

/// Build a frequency histogram from returns for entropy computation.
/// Returns counts suitable for Shannon entropy.
fn return_histogram(returns: &[Rational], num_bins: usize) -> Vec<u64> {
    if returns.is_empty() || num_bins == 0 {
        return vec![];
    }
    let mut counts = vec![0u64; num_bins];
    for r in returns {
        let scaled = r.clone() * Rational::from((10000i64, 1i64));
        let n = scaled.numer().clone().abs();
        let bucket = (n % rug::Integer::from(num_bins as u64)).to_u32_wrapping() as usize;
        counts[bucket] += 1;
    }
    // Filter out zero counts for entropy
    counts.into_iter().filter(|&c| c > 0).collect()
}

/// Analyze a single asset's price series using QuantaBeta exact arithmetic.
pub fn analyze_asset(symbol: &str, prices: &[f64], window: usize) -> AssetAnalysis {
    let rational_prices = crate::convert::prices_to_rationals(prices);
    let returns = compute_returns(&rational_prices);

    // Partition volatility
    let vol_rationals = quantabeta_core::features::compute_partition_volatility(&returns, window);
    let partition_volatility: Vec<f64> = vol_rationals.iter().map(|r| rational_to_f64(r)).collect();

    // Build histogram for entropy
    let hist = return_histogram(&returns, window.max(2));

    // Shannon entropy (point estimate)
    let entropy_point = if !hist.is_empty() {
        let h = true_entropy::shannon_entropy_exact(hist.clone(), 256);
        h.to_f64()
    } else {
        0.0
    };

    // Shannon entropy (interval)
    let entropy_interval = if !hist.is_empty() {
        let (lo, hi) = true_entropy::shannon_entropy_interval(hist, 256);
        (lo.to_f64(), hi.to_f64())
    } else {
        (0.0, 0.0)
    };

    // Complexity invariant
    let complexity = ramanujan_ops::partition::complexity_invariant(prices.len().min(100));
    let complexity_bound = complexity.to_string();

    // Ramanujan congruence check
    let max_k = (prices.len() / 5).max(1).min(20);
    let ramanujan_congruence_holds = ramanujan_ops::partition::verify_ramanujan_congruence_5(max_k);

    // Audit hash: deterministic hash of all inputs for reproducibility
    let mut hasher = Sha256::new();
    hasher.update(symbol.as_bytes());
    for p in prices {
        hasher.update(p.to_le_bytes());
    }
    hasher.update(window.to_le_bytes());
    let audit_hash = hex::encode(hasher.finalize());

    AssetAnalysis {
        symbol: symbol.to_string(),
        partition_volatility,
        entropy_point,
        entropy_interval,
        complexity_bound,
        ramanujan_congruence_holds,
        data_points: prices.len(),
        audit_hash,
    }
}

/// Compute Hecke cross-correlation between two assets.
pub fn cross_correlate(
    symbol_a: &str,
    prices_a: &[f64],
    symbol_b: &str,
    prices_b: &[f64],
    level: u32,
) -> CrossCorrelation {
    let rat_a = crate::convert::prices_to_rationals(prices_a);
    let rat_b = crate::convert::prices_to_rationals(prices_b);

    let returns_a = compute_returns(&rat_a);
    let returns_b = compute_returns(&rat_b);

    let corr = quantabeta_core::features::hecke_cross_correlation(&returns_a, &returns_b, level);

    CrossCorrelation {
        asset_a: symbol_a.to_string(),
        asset_b: symbol_b.to_string(),
        hecke_correlation: rational_to_f64(&corr),
        level,
    }
}

/// Build a full QuantaBeta report for a portfolio.
pub fn build_report(portfolio: &[(String, Vec<f64>)], window: usize) -> QuantaBetaReport {
    let timestamp = chrono::Utc::now().to_rfc3339();

    let assets: Vec<AssetAnalysis> = portfolio
        .iter()
        .map(|(sym, prices)| analyze_asset(sym, prices, window))
        .collect();

    // Cross-correlations for all pairs
    let mut cross_correlations = Vec::new();
    for i in 0..portfolio.len() {
        for j in (i + 1)..portfolio.len() {
            let cc = cross_correlate(
                &portfolio[i].0,
                &portfolio[i].1,
                &portfolio[j].0,
                &portfolio[j].1,
                2, // default Hecke level
            );
            cross_correlations.push(cc);
        }
    }

    // Report hash
    let mut hasher = Sha256::new();
    hasher.update(timestamp.as_bytes());
    for a in &assets {
        hasher.update(a.audit_hash.as_bytes());
    }
    let report_hash = hex::encode(hasher.finalize());

    QuantaBetaReport {
        assets,
        cross_correlations,
        timestamp,
        report_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_asset_basic() {
        let prices: Vec<f64> = (0..30)
            .map(|i| 0.50 + 0.01 * (i as f64 * 0.3).sin())
            .collect();
        let report = analyze_asset("XRP", &prices, 5);
        assert_eq!(report.symbol, "XRP");
        assert!(!report.partition_volatility.is_empty());
        assert!(report.entropy_point >= 0.0);
        assert!(report.ramanujan_congruence_holds);
        assert_eq!(report.data_points, 30);
    }

    #[test]
    fn test_build_report() {
        let portfolio = vec![
            (
                "XRP".to_string(),
                (0..30)
                    .map(|i| 0.50 + 0.01 * (i as f64 * 0.3).sin())
                    .collect(),
            ),
            (
                "DOGE".to_string(),
                (0..30)
                    .map(|i| 0.10 + 0.005 * (i as f64 * 0.5).cos())
                    .collect(),
            ),
        ];
        let report = build_report(&portfolio, 5);
        assert_eq!(report.assets.len(), 2);
        assert_eq!(report.cross_correlations.len(), 1);
    }

    #[test]
    fn test_analyze_asset_deterministic() {
        let prices: Vec<f64> = (0..20).map(|i| 1.0 + 0.01 * i as f64).collect();
        let r1 = analyze_asset("TEST", &prices, 5);
        let r2 = analyze_asset("TEST", &prices, 5);
        assert_eq!(r1.partition_volatility, r2.partition_volatility);
        assert_eq!(r1.audit_hash, r2.audit_hash);
    }
}
