use rug::Rational;

/// Convert an f64 price to exact Rational.
pub fn f64_to_rational(val: f64) -> Rational {
    Rational::from_f64(val).unwrap_or_else(|| Rational::from((0i64, 1i64)))
}

/// Convert a slice of f64 prices to Vec<Rational>.
pub fn prices_to_rationals(prices: &[f64]) -> Vec<Rational> {
    prices.iter().map(|&p| f64_to_rational(p)).collect()
}

/// Convert a Rational back to f64 for display purposes.
pub fn rational_to_f64(r: &Rational) -> f64 {
    r.to_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_f64_to_rational_roundtrip() {
        let price = 0.585f64;
        let r = f64_to_rational(price);
        let back = rational_to_f64(&r);
        assert_eq!(price, back);
    }

    #[test]
    fn test_prices_to_rationals() {
        let prices = vec![100.0, 101.5, 99.25];
        let rats = prices_to_rationals(&prices);
        assert_eq!(rats.len(), 3);
        assert_eq!(rational_to_f64(&rats[0]), 100.0);
    }

    #[test]
    fn test_nan_handled() {
        let r = f64_to_rational(f64::NAN);
        assert_eq!(rational_to_f64(&r), 0.0);
    }

    #[test]
    fn test_infinity_handled() {
        let r = f64_to_rational(f64::INFINITY);
        assert_eq!(rational_to_f64(&r), 0.0);
    }
}
