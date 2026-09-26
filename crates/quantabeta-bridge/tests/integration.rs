use quantabeta_bridge::analysis::{analyze_asset, build_report, cross_correlate};
use quantabeta_bridge::cache::{get_cached_report, init_cache_table, store_report};
use quantabeta_bridge::convert::{f64_to_rational, rational_to_f64};

fn sample_xrp_prices() -> Vec<f64> {
    vec![
        0.50, 0.51, 0.49, 0.52, 0.53, 0.51, 0.54, 0.52, 0.55, 0.53, 0.56, 0.54, 0.57, 0.55,
        0.58, 0.56, 0.59, 0.57, 0.60, 0.58, 0.61, 0.59, 0.62, 0.60, 0.63, 0.61, 0.64, 0.62,
        0.65, 0.63,
    ]
}

fn sample_doge_prices() -> Vec<f64> {
    vec![
        0.10, 0.11, 0.09, 0.12, 0.10, 0.11, 0.13, 0.10, 0.12, 0.11, 0.10, 0.12, 0.11, 0.13,
        0.12, 0.10, 0.11, 0.13, 0.12, 0.14, 0.13, 0.11, 0.12, 0.14, 0.13, 0.15, 0.14, 0.12,
        0.13, 0.15,
    ]
}

fn sample_portfolio() -> Vec<(String, Vec<f64>)> {
    vec![
        ("XRP".to_string(), sample_xrp_prices()),
        ("DOGE".to_string(), sample_doge_prices()),
    ]
}

#[test]
fn test_full_pipeline_f64_to_report() {
    let portfolio = sample_portfolio();
    let report = build_report(&portfolio, 5);

    // Verify report structure
    assert_eq!(report.assets.len(), 2);
    assert_eq!(report.cross_correlations.len(), 1);
    assert!(report.assets[0].ramanujan_congruence_holds);
    assert!(report.assets[1].ramanujan_congruence_holds);

    // Entropy interval sanity: lo <= point <= hi
    for asset in &report.assets {
        assert!(
            asset.entropy_interval.0 <= asset.entropy_point,
            "{}: entropy lower bound {} > point {}",
            asset.symbol,
            asset.entropy_interval.0,
            asset.entropy_point
        );
        assert!(
            asset.entropy_point <= asset.entropy_interval.1,
            "{}: entropy point {} > upper bound {}",
            asset.symbol,
            asset.entropy_point,
            asset.entropy_interval.1
        );
    }

    // Cache roundtrip
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    init_cache_table(&conn).unwrap();
    store_report(&conn, &report, "test_hash").unwrap();
    let cached = get_cached_report(&conn, "test_hash").unwrap();
    assert_eq!(cached.assets.len(), 2);
    assert_eq!(cached.report_hash, report.report_hash);

    // JSON serialization
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("XRP"));
    assert!(json.contains("DOGE"));
    assert!(json.contains("partition_volatility"));
    assert!(json.contains("hecke_correlation"));

    // Determinism: same inputs produce same numeric results
    // (report_hash will differ because it includes the timestamp)
    let portfolio2 = sample_portfolio();
    let report2 = build_report(&portfolio2, 5);
    assert_eq!(
        report.assets[0].partition_volatility,
        report2.assets[0].partition_volatility
    );
    assert_eq!(report.assets[0].entropy_point, report2.assets[0].entropy_point);
    assert_eq!(report.assets[0].audit_hash, report2.assets[0].audit_hash);
}

#[test]
fn test_conversion_preserves_precision() {
    let test_prices = vec![0.585, 1.05, 100.25, 0.001, 99999.99];
    for price in test_prices {
        let rational = f64_to_rational(price);
        let back = rational_to_f64(&rational);
        assert_eq!(price, back, "precision lost for {}", price);
    }
}

#[test]
fn test_analyze_single_asset() {
    let prices = sample_xrp_prices();
    let analysis = analyze_asset("XRP", &prices, 5);
    assert_eq!(analysis.symbol, "XRP");
    assert_eq!(analysis.data_points, 30);
    assert!(!analysis.partition_volatility.is_empty());
    assert!(!analysis.audit_hash.is_empty());
}

#[test]
fn test_cross_correlate_pair() {
    let xrp = sample_xrp_prices();
    let doge = sample_doge_prices();
    let cc = cross_correlate("XRP", &xrp, "DOGE", &doge, 2);
    assert_eq!(cc.asset_a, "XRP");
    assert_eq!(cc.asset_b, "DOGE");
    assert_eq!(cc.level, 2);
    // Correlation should be a finite number
    assert!(cc.hecke_correlation.is_finite());
}

#[test]
fn test_cache_miss_returns_none() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    init_cache_table(&conn).unwrap();
    assert!(get_cached_report(&conn, "nonexistent").is_none());
}
