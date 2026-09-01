use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub finnhub_key: String,
    pub ntfy_topic: String,
    pub gmail_address: String,
    pub gmail_app_password: String,
    pub digest_to: String,
    pub timezone: String,
    pub poll_seconds: u64,
    pub alert_cooldown_minutes: u64,
    pub hysteresis_pct: f64,
    pub db_path: PathBuf,
    pub worm_db_path: PathBuf,
    pub redis_url: Option<String>,
    pub prolog_url: Option<String>,
}

impl Config {
    pub fn load(project_dir: &std::path::Path) -> crate::Result<Self> {
        let env_path = project_dir.join(".env");
        if env_path.exists() {
            dotenvy::from_path(&env_path)
                .map_err(|e| crate::VigilError::Config(format!("Failed to load .env: {e}")))?;
        }

        Ok(Self {
            finnhub_key: std::env::var("FINNHUB_KEY").unwrap_or_default(),
            ntfy_topic: std::env::var("NTFY_TOPIC").unwrap_or_default(),
            gmail_address: std::env::var("GMAIL_ADDRESS").unwrap_or_default(),
            gmail_app_password: std::env::var("GMAIL_APP_PASSWORD").unwrap_or_default(),
            digest_to: std::env::var("DIGEST_TO")
                .unwrap_or_else(|_| std::env::var("GMAIL_ADDRESS").unwrap_or_default()),
            timezone: std::env::var("TIMEZONE")
                .unwrap_or_else(|_| "America/Los_Angeles".to_string()),
            poll_seconds: std::env::var("POLL_SECONDS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .unwrap_or(60),
            alert_cooldown_minutes: std::env::var("ALERT_COOLDOWN_MINUTES")
                .unwrap_or_else(|_| "45".to_string())
                .parse()
                .unwrap_or(45),
            hysteresis_pct: std::env::var("HYSTERESIS_PCT")
                .unwrap_or_else(|_| "0.75".to_string())
                .parse()
                .unwrap_or(0.75),
            db_path: project_dir.join("robnhud.db"),
            worm_db_path: project_dir.join("worm.db"),
            redis_url: std::env::var("REDIS_URL").ok(),
            prolog_url: std::env::var("PROLOG_URL").ok(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_config_defaults() {
        std::env::remove_var("FINNHUB_KEY");
        std::env::remove_var("POLL_SECONDS");
        std::env::remove_var("TIMEZONE");

        let config = Config::load(Path::new("/nonexistent")).unwrap();
        assert_eq!(config.poll_seconds, 60);
        assert_eq!(config.timezone, "America/Los_Angeles");
        assert_eq!(config.hysteresis_pct, 0.75);
        assert!(config.redis_url.is_none());
    }
}
