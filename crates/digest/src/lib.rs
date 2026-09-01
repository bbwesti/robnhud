//! CRYPTEK VIGIL — Daily Digest
//!
//! HTML email summary with portfolio data, regime, scenarios, news relevance.

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use vigil_core::{Holding, NewsItem, PriceSnapshot};
use vigil_quant::ScenarioBands;
use vigil_regime::Regime;

pub fn analyze_relevance(title: &str, holdings: &[Holding]) -> Option<String> {
    let title_upper = title.to_uppercase();
    let mentioned: Vec<&str> = holdings.iter()
        .filter(|h| title_upper.contains(&h.symbol.to_uppercase()))
        .map(|h| h.symbol.as_str()).collect();
    if !mentioned.is_empty() {
        return Some(format!("DIRECTLY ABOUT YOUR {} HOLDING", mentioned.join(" + ")));
    }
    let crypto_keywords = ["CRYPTO", "BITCOIN", "BLOCKCHAIN", "DEFI", "NFT", "WEB3", "STABLECOIN"];
    let has_crypto = holdings.iter().any(|h| h.asset_class == vigil_core::AssetClass::Crypto);
    if has_crypto && crypto_keywords.iter().any(|k| title_upper.contains(k)) {
        return Some("CRYPTO MARKET — affects your crypto holdings".to_string());
    }
    let reg_keywords = ["SEC", "REGULATION", "REGULATORY", "COMPLIANCE", "ENFORCEMENT"];
    if has_crypto && reg_keywords.iter().any(|k| title_upper.contains(k)) {
        return Some("REGULATION — SEC/regulatory news can move all crypto prices".to_string());
    }
    let macro_keywords = ["FED", "FEDERAL RESERVE", "INTEREST RATE", "INFLATION", "GDP", "EMPLOYMENT"];
    if macro_keywords.iter().any(|k| title_upper.contains(k)) {
        return Some("MACRO — rate/economic decisions move all asset classes".to_string());
    }
    let index_keywords = ["S&P", "NASDAQ", "DOW", "NIKKEI", "DAX", "KOSPI"];
    if index_keywords.iter().any(|k| title_upper.contains(k)) {
        return Some("INDEX TRACKING — global market movement".to_string());
    }
    None
}

pub fn build_digest_html(
    holdings: &[Holding], snapshot: &PriceSnapshot, regime: Regime,
    scenarios: &[ScenarioBands], news: &[NewsItem], worm_block_count: u64, worm_valid: bool,
) -> String {
    let mut html = String::from(
        r#"<html><body style="background:#0a0a0a;color:#c0c0c0;font-family:monospace;padding:20px;">
<h1 style="color:#00ff66;font-size:24px;letter-spacing:4px;">CRYPTEK VIGIL — DAILY DIGEST</h1>
<p style="color:#8b7d3c;">Dynasty Wealth Monitoring Protocol</p>
<hr style="border-color:#1a3a1a;">"#,
    );
    let mut total_value = 0.0;
    let mut total_cost = 0.0;
    html.push_str(r#"<h2 style="color:#8b7d3c;font-size:14px;letter-spacing:3px;">DYNASTY VAULT</h2>"#);
    html.push_str(r#"<table style="width:100%;border-collapse:collapse;">"#);
    html.push_str(r#"<tr style="color:#3a6a3a;font-size:12px;"><td>Symbol</td><td>Price</td><td>24h</td><td>Qty</td><td>Value</td><td>P/L</td></tr>"#);
    for h in holdings {
        if let Some(p) = snapshot.prices.get(&h.symbol) {
            let value = p.price * h.qty;
            let cost = h.avg_cost * h.qty;
            total_value += value;
            total_cost += cost;
            let pl = value - cost;
            let change_color = if p.change_pct >= 0.0 { "#00ff66" } else { "#ff3333" };
            let pl_color = if pl >= 0.0 { "#00ff66" } else { "#ff3333" };
            html.push_str(&format!(
                r#"<tr style="border-bottom:1px solid #1a1a1a;font-size:12px;">
                <td style="color:#00cc55;padding:4px 0;">{}</td>
                <td>${:.4}</td>
                <td style="color:{};">{:+.1}%</td>
                <td>{:.4}</td>
                <td>${:.2}</td>
                <td style="color:{};">{:+.2}</td></tr>"#,
                h.symbol, p.price, change_color, p.change_pct, h.qty, value, pl_color, pl
            ));
        }
    }
    let total_pl = total_value - total_cost;
    let total_pct = if total_cost > 0.0 { (total_pl / total_cost) * 100.0 } else { 0.0 };
    html.push_str(&format!(
        r#"<tr style="border-top:2px solid #1a3a1a;font-size:14px;color:#00ff66;">
        <td><b>TOTAL</b></td><td></td><td></td><td></td>
        <td><b>${:.2}</b></td><td><b>{:+.2} ({:+.1}%)</b></td></tr>"#,
        total_value, total_pl, total_pct
    ));
    html.push_str("</table>");
    let regime_color = match regime {
        Regime::Bull => "#00ff66", Regime::Bear => "#ff3333", Regime::Transition => "#ff9900", Regime::Unknown => "#808080",
    };
    html.push_str(&format!(
        r#"<h2 style="color:#8b7d3c;font-size:14px;letter-spacing:3px;margin-top:20px;">REGIME</h2>
        <p style="color:{};font-size:18px;letter-spacing:2px;">{}</p>"#,
        regime_color, regime
    ));
    html.push_str(r#"<h2 style="color:#8b7d3c;font-size:14px;letter-spacing:3px;margin-top:20px;">SCENARIO BANDS</h2>"#);
    for sb in scenarios {
        html.push_str(&format!(r#"<p style="color:#00cc55;">{}</p>"#, sb.symbol));
        for band in &sb.bands {
            html.push_str(&format!(
                r#"<p style="font-size:11px;color:#808080;">  {} — ${:.4} to ${:.4} ({})</p>"#,
                band.label, band.range.0, band.range.1, band.probability
            ));
        }
    }
    html.push_str(r#"<p style="font-size:10px;color:#4a4a4a;">SCENARIO BANDS — NOT PREDICTIONS — Human Review Required</p>"#);
    html.push_str(r#"<h2 style="color:#8b7d3c;font-size:14px;letter-spacing:3px;margin-top:20px;">NEWS & HOW IT AFFECTS YOUR PORTFOLIO</h2>"#);
    for item in news {
        let relevance = analyze_relevance(&item.title, holdings).unwrap_or_default();
        html.push_str(&format!(
            r#"<p style="font-size:12px;border-bottom:1px solid #1a1a1a;padding:6px 0;">
            {} <br><span style="color:#00cc55;font-size:10px;">{}</span></p>"#,
            item.title, relevance
        ));
    }
    html.push_str(&format!(
        r#"<h2 style="color:#8b7d3c;font-size:14px;letter-spacing:3px;margin-top:20px;">BIFROST WORM CHAIN</h2>
        <p style="font-size:11px;">Blocks: {} | Integrity: {}</p>"#,
        worm_block_count, if worm_valid { "VERIFIED" } else { "BROKEN" }
    ));
    html.push_str(r#"<hr style="border-color:#1a3a1a;margin-top:20px;">
    <p style="font-size:10px;color:#3a3a3a;text-align:center;">
    CRYPTEK VIGIL v2.0 — <a href="https://github.com/bbwesti/robnhud" style="color:#3a6a3a;">github.com/bbwesti/robnhud</a>
    </p></body></html>"#);
    html
}

pub fn send_email(from: &str, to: &str, app_password: &str, subject: &str, html_body: &str) -> Result<(), String> {
    if from.is_empty() || app_password.is_empty() {
        return Err("Gmail credentials not configured".to_string());
    }
    let email = Message::builder()
        .from(from.parse().map_err(|e| format!("Invalid from address: {e}"))?)
        .to(to.parse().map_err(|e| format!("Invalid to address: {e}"))?)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(html_body.to_string())
        .map_err(|e| format!("Failed to build email: {e}"))?;
    let creds = Credentials::new(from.to_string(), app_password.to_string());
    let mailer = SmtpTransport::relay("smtp.gmail.com")
        .map_err(|e| format!("SMTP relay error: {e}"))?
        .credentials(creds).build();
    mailer.send(&email).map_err(|e| format!("Send failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vigil_core::AssetClass;

    #[test]
    fn test_relevance_direct_mention() {
        let holdings = vec![Holding { symbol: "XRP".to_string(), asset_class: AssetClass::Crypto, qty: 100.0, avg_cost: 1.0 }];
        let result = analyze_relevance("Ripple XRP wins SEC case", &holdings);
        assert!(result.unwrap().contains("XRP"));
    }

    #[test]
    fn test_relevance_crypto_keyword() {
        let holdings = vec![Holding { symbol: "XRP".to_string(), asset_class: AssetClass::Crypto, qty: 100.0, avg_cost: 1.0 }];
        let result = analyze_relevance("Bitcoin hits new all-time high", &holdings);
        assert!(result.unwrap().contains("CRYPTO"));
    }

    #[test]
    fn test_relevance_macro() {
        let holdings = vec![Holding { symbol: "AAPL".to_string(), asset_class: AssetClass::Equity, qty: 10.0, avg_cost: 150.0 }];
        let result = analyze_relevance("Fed raises interest rate by 25bps", &holdings);
        assert!(result.unwrap().contains("MACRO"));
    }

    #[test]
    fn test_relevance_none() {
        let holdings = vec![Holding { symbol: "XRP".to_string(), asset_class: AssetClass::Crypto, qty: 100.0, avg_cost: 1.0 }];
        let result = analyze_relevance("Weather forecast for next week", &holdings);
        assert!(result.is_none());
    }
}
