use crate::pricing::calculator::ModelPrice;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub subscription: Vec<SubscriptionPeriod>,
    #[serde(default)]
    pub pricing_override: HashMap<String, PriceOverride>,
}

#[derive(Debug, Deserialize)]
pub struct SubscriptionPeriod {
    pub start_date: String,
    pub monthly_price_usd: f64,
    pub plan: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PriceOverride {
    pub base_input: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
    pub output: f64,
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn to_model_prices(&self) -> HashMap<String, ModelPrice> {
        self.pricing_override
            .iter()
            .map(|(k, v)| {
                (k.clone(), ModelPrice {
                    base_input: v.base_input,
                    cache_write_5m: v.cache_write_5m,
                    cache_write_1h: v.cache_write_1h,
                    cache_read: v.cache_read,
                    output: v.output,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config() {
        let toml_str = r#"
[[subscription]]
start_date = "2026-01-01"
monthly_price_usd = 100.0
plan = "max_5x"

[[subscription]]
start_date = "2026-03-01"
monthly_price_usd = 200.0
plan = "max_20x"

[pricing_override.claude-opus-4-6]
base_input = 5.0
cache_write_5m = 6.25
cache_write_1h = 10.0
cache_read = 0.50
output = 25.0
"#;

        let config: Config = toml::from_str(toml_str).unwrap();

        assert_eq!(config.subscription.len(), 2);
        assert_eq!(config.subscription[0].start_date, "2026-01-01");
        assert!((config.subscription[0].monthly_price_usd - 100.0).abs() < f64::EPSILON);
        assert_eq!(config.subscription[0].plan.as_deref(), Some("max_5x"));
        assert_eq!(config.subscription[1].start_date, "2026-03-01");
        assert!((config.subscription[1].monthly_price_usd - 200.0).abs() < f64::EPSILON);
        assert_eq!(config.subscription[1].plan.as_deref(), Some("max_20x"));

        assert_eq!(config.pricing_override.len(), 1);
        let opus = &config.pricing_override["claude-opus-4-6"];
        assert!((opus.base_input - 5.0).abs() < f64::EPSILON);
        assert!((opus.cache_write_5m - 6.25).abs() < f64::EPSILON);
        assert!((opus.cache_write_1h - 10.0).abs() < f64::EPSILON);
        assert!((opus.cache_read - 0.50).abs() < f64::EPSILON);
        assert!((opus.output - 25.0).abs() < f64::EPSILON);

        let model_prices = config.to_model_prices();
        assert_eq!(model_prices.len(), 1);
        let mp = &model_prices["claude-opus-4-6"];
        assert!((mp.base_input - 5.0).abs() < f64::EPSILON);
        assert!((mp.output - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn empty_config() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.subscription.is_empty());
        assert!(config.pricing_override.is_empty());
        assert!(config.to_model_prices().is_empty());
    }
}
