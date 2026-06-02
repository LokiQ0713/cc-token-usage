use std::collections::HashMap;

use chrono::NaiveDate;

use crate::data::models::TokenUsage;

/// Date when pricing data was fetched from Anthropic's official pricing page.
pub const PRICING_FETCH_DATE: &str = "2026-03-21";
/// Source URL for pricing data.
pub const PRICING_SOURCE: &str = "platform.claude.com/docs/en/about-claude/pricing";

/// Built-in model used to price unknown models when no exact/prefix match exists.
/// Updated whenever a new "latest" Claude flagship is added to `builtin_prices()`.
pub const LATEST_FALLBACK_MODEL: &str = "claude-opus-4-8";

// ─── Data Structures ─────────────────────────────────────────────────────────

/// Per-model pricing in dollars per million tokens.
#[derive(Debug, Clone)]
pub struct ModelPrice {
    /// Base input price ($/MTok).
    pub base_input: f64,
    /// Cache write price for 5-minute ephemeral TTL ($/MTok).
    pub cache_write_5m: f64,
    /// Cache write price for 1-hour ephemeral TTL ($/MTok).
    pub cache_write_1h: f64,
    /// Cache read price ($/MTok).
    pub cache_read: f64,
    /// Output price ($/MTok).
    pub output: f64,
}

/// Itemised cost breakdown for a single turn.
#[derive(Debug, Clone)]
pub struct CostBreakdown {
    pub input_cost: f64,
    pub cache_write_5m_cost: f64,
    pub cache_write_1h_cost: f64,
    pub cache_read_cost: f64,
    pub output_cost: f64,
    pub total: f64,
    pub price_source: PriceSource,
}

/// Where the pricing data came from.
#[derive(Debug, Clone, PartialEq)]
pub enum PriceSource {
    /// Hardcoded in the binary.
    Builtin,
    /// Loaded from a user config file override.
    Config,
    /// Unknown model — priced using the latest built-in Claude as a stand-in.
    /// `requested` is the model name actually queried; `fallback_to` is the
    /// built-in entry whose prices were used.
    Fallback {
        requested: String,
        fallback_to: String,
    },
    /// Model not found and no fallback available – all costs are zero.
    Unknown,
}

// ─── Built-in Price Table ────────────────────────────────────────────────────

fn builtin_prices() -> HashMap<String, ModelPrice> {
    let entries: Vec<(&str, ModelPrice)> = vec![
        (
            "claude-opus-4-8",
            ModelPrice {
                base_input: 5.0,
                cache_write_5m: 6.25,
                cache_write_1h: 10.0,
                cache_read: 0.50,
                output: 25.0,
            },
        ),
        (
            "claude-opus-4-7",
            ModelPrice {
                base_input: 5.0,
                cache_write_5m: 6.25,
                cache_write_1h: 10.0,
                cache_read: 0.50,
                output: 25.0,
            },
        ),
        (
            "claude-opus-4-6",
            ModelPrice {
                base_input: 5.0,
                cache_write_5m: 6.25,
                cache_write_1h: 10.0,
                cache_read: 0.50,
                output: 25.0,
            },
        ),
        (
            "claude-opus-4-5",
            ModelPrice {
                base_input: 5.0,
                cache_write_5m: 6.25,
                cache_write_1h: 10.0,
                cache_read: 0.50,
                output: 25.0,
            },
        ),
        (
            "claude-opus-4-1",
            ModelPrice {
                base_input: 15.0,
                cache_write_5m: 18.75,
                cache_write_1h: 30.0,
                cache_read: 1.50,
                output: 75.0,
            },
        ),
        (
            "claude-opus-4",
            ModelPrice {
                base_input: 15.0,
                cache_write_5m: 18.75,
                cache_write_1h: 30.0,
                cache_read: 1.50,
                output: 75.0,
            },
        ),
        (
            "claude-sonnet-4-6",
            ModelPrice {
                base_input: 3.0,
                cache_write_5m: 3.75,
                cache_write_1h: 6.0,
                cache_read: 0.30,
                output: 15.0,
            },
        ),
        (
            "claude-sonnet-4-5",
            ModelPrice {
                base_input: 3.0,
                cache_write_5m: 3.75,
                cache_write_1h: 6.0,
                cache_read: 0.30,
                output: 15.0,
            },
        ),
        (
            "claude-sonnet-4",
            ModelPrice {
                base_input: 3.0,
                cache_write_5m: 3.75,
                cache_write_1h: 6.0,
                cache_read: 0.30,
                output: 15.0,
            },
        ),
        (
            "claude-haiku-4-5",
            ModelPrice {
                base_input: 1.0,
                cache_write_5m: 1.25,
                cache_write_1h: 2.0,
                cache_read: 0.10,
                output: 5.0,
            },
        ),
        (
            "claude-haiku-3-5",
            ModelPrice {
                base_input: 0.80,
                cache_write_5m: 1.0,
                cache_write_1h: 1.60,
                cache_read: 0.08,
                output: 4.0,
            },
        ),
        (
            "claude-3-haiku",
            ModelPrice {
                base_input: 0.25,
                cache_write_5m: 0.30,
                cache_write_1h: 0.50,
                cache_read: 0.03,
                output: 1.25,
            },
        ),
    ];

    entries
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

// ─── Calculator ──────────────────────────────────────────────────────────────

/// Pricing calculator with built-in prices and optional config overrides.
pub struct PricingCalculator {
    prices: HashMap<String, ModelPrice>,
    overrides: HashMap<String, ModelPrice>,
}

impl Default for PricingCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl PricingCalculator {
    /// Create a new calculator initialised with built-in prices.
    pub fn new() -> Self {
        Self {
            prices: builtin_prices(),
            overrides: HashMap::new(),
        }
    }

    /// Set config-file price overrides. These take priority over built-in prices.
    pub fn with_overrides(mut self, overrides: HashMap<String, ModelPrice>) -> Self {
        self.overrides = overrides;
        self
    }

    /// Look up the price for a model.
    ///
    /// Resolution order:
    /// 1. Exact match in overrides
    /// 2. Prefix match in overrides
    /// 3. Exact match in built-in prices
    /// 4. Prefix match in built-in prices
    /// 5. Fallback to the latest built-in Claude (returns `PriceSource::Fallback`)
    pub fn get_price(&self, model: &str) -> Option<(&ModelPrice, PriceSource)> {
        // Strip any trailing context-window suffix in square brackets, e.g.
        // `claude-opus-4-8[1m]` → `claude-opus-4-8`. This is purely a routing
        // affix appended by Claude Code to mark the active context window; it is
        // not part of the priced model identity. Without stripping it, the
        // bracketed name would miss the exact builtin entry and prefix-match an
        // older generation (e.g. `claude-opus-4`), ~3x over-pricing the turn.
        let model = match model.split_once('[') {
            Some((base, rest)) if rest.ends_with(']') => base,
            _ => model,
        };

        // 1. Exact override
        if let Some(p) = self.overrides.get(model) {
            return Some((p, PriceSource::Config));
        }
        // 2. Prefix override (longest prefix wins)
        if let Some(p) = Self::prefix_lookup(&self.overrides, model) {
            return Some((p, PriceSource::Config));
        }
        // 3. Exact built-in
        if let Some(p) = self.prices.get(model) {
            return Some((p, PriceSource::Builtin));
        }
        // 4. Prefix built-in
        if let Some(p) = Self::prefix_lookup(&self.prices, model) {
            return Some((p, PriceSource::Builtin));
        }
        // 5. Fallback to latest built-in Claude so unknown models don't
        // silently produce $0 costs. Caller can detect via `PriceSource::Fallback`.
        if let Some((fallback_key, fallback_price)) = self.latest_builtin_claude() {
            return Some((
                fallback_price,
                PriceSource::Fallback {
                    requested: model.to_string(),
                    fallback_to: fallback_key.to_string(),
                },
            ));
        }
        None
    }

    /// Find the entry whose key is the longest prefix of `model`.
    fn prefix_lookup<'a>(
        map: &'a HashMap<String, ModelPrice>,
        model: &str,
    ) -> Option<&'a ModelPrice> {
        map.iter()
            .filter(|(key, _)| model.starts_with(key.as_str()))
            .max_by_key(|(key, _)| key.len())
            .map(|(_, v)| v)
    }

    /// Look up the built-in entry used as the unknown-model fallback.
    ///
    /// Returns `None` only if `LATEST_FALLBACK_MODEL` is somehow missing from
    /// the built-in table — guarded by a unit test.
    fn latest_builtin_claude(&self) -> Option<(&str, &ModelPrice)> {
        self.prices
            .get_key_value(LATEST_FALLBACK_MODEL)
            .map(|(k, v)| (k.as_str(), v))
    }

    /// Calculate the cost of a single assistant turn.
    pub fn calculate_turn_cost(&self, model: &str, usage: &TokenUsage) -> CostBreakdown {
        let (price, source) = match self.get_price(model) {
            Some((p, s)) => (p, s),
            None => {
                return CostBreakdown {
                    input_cost: 0.0,
                    cache_write_5m_cost: 0.0,
                    cache_write_1h_cost: 0.0,
                    cache_read_cost: 0.0,
                    output_cost: 0.0,
                    total: 0.0,
                    price_source: PriceSource::Unknown,
                };
            }
        };

        let input_mtok = usage.input_tokens.unwrap_or(0) as f64 / 1_000_000.0;
        let output_mtok = usage.output_tokens.unwrap_or(0) as f64 / 1_000_000.0;
        let cache_read_mtok = usage.cache_read_input_tokens.unwrap_or(0) as f64 / 1_000_000.0;

        // Distinguish 5m and 1h cache write buckets
        let (cw_5m, cw_1h) = match &usage.cache_creation {
            Some(detail) => (
                detail.ephemeral_5m_input_tokens.unwrap_or(0) as f64 / 1_000_000.0,
                detail.ephemeral_1h_input_tokens.unwrap_or(0) as f64 / 1_000_000.0,
            ),
            None => {
                // No breakdown available – treat everything as 5m (conservative estimate)
                let total_cw = usage.cache_creation_input_tokens.unwrap_or(0) as f64 / 1_000_000.0;
                (total_cw, 0.0)
            }
        };

        let input_cost = input_mtok * price.base_input;
        let cache_write_5m_cost = cw_5m * price.cache_write_5m;
        let cache_write_1h_cost = cw_1h * price.cache_write_1h;
        let cache_read_cost = cache_read_mtok * price.cache_read;
        let output_cost = output_mtok * price.output;

        let total =
            input_cost + cache_write_5m_cost + cache_write_1h_cost + cache_read_cost + output_cost;

        CostBreakdown {
            input_cost,
            cache_write_5m_cost,
            cache_write_1h_cost,
            cache_read_cost,
            output_cost,
            total,
            price_source: source,
        }
    }

    /// Number of days since the built-in pricing data was fetched.
    pub fn pricing_age_days() -> i64 {
        let fetch_date =
            NaiveDate::parse_from_str(PRICING_FETCH_DATE, "%Y-%m-%d").expect("valid date constant");
        let today = chrono::Utc::now().date_naive();
        (today - fetch_date).num_days()
    }

    /// Returns `true` if the built-in pricing data is older than 90 days.
    pub fn is_pricing_stale() -> bool {
        Self::pricing_age_days() > 90
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::{CacheCreationDetail, TokenUsage};

    /// Helper to build a `TokenUsage` for testing.
    fn make_usage(
        input: u64,
        output: u64,
        cache_create: u64,
        cache_read: u64,
        cw_5m: u64,
        cw_1h: u64,
    ) -> TokenUsage {
        let cache_creation = if cw_5m > 0 || cw_1h > 0 {
            Some(CacheCreationDetail {
                ephemeral_5m_input_tokens: Some(cw_5m),
                ephemeral_1h_input_tokens: Some(cw_1h),
            })
        } else {
            None
        };

        TokenUsage {
            input_tokens: Some(input),
            output_tokens: Some(output),
            cache_creation_input_tokens: Some(cache_create),
            cache_read_input_tokens: Some(cache_read),
            cache_creation,
            server_tool_use: None,
            service_tier: None,
            speed: None,
            inference_geo: None,
        }
    }

    #[test]
    fn opus_46_pricing() {
        let calc = PricingCalculator::new();
        // 1M input + 1M output + 1M cache_write_5m + 1M cache_read
        let usage = make_usage(1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 0);
        let cost = calc.calculate_turn_cost("claude-opus-4-6", &usage);

        assert!(
            (cost.input_cost - 5.0).abs() < 1e-9,
            "input_cost: {}",
            cost.input_cost
        );
        assert!(
            (cost.cache_write_5m_cost - 6.25).abs() < 1e-9,
            "cache_write_5m_cost: {}",
            cost.cache_write_5m_cost
        );
        assert!(
            (cost.cache_write_1h_cost - 0.0).abs() < 1e-9,
            "cache_write_1h_cost: {}",
            cost.cache_write_1h_cost
        );
        assert!(
            (cost.cache_read_cost - 0.50).abs() < 1e-9,
            "cache_read_cost: {}",
            cost.cache_read_cost
        );
        assert!(
            (cost.output_cost - 25.0).abs() < 1e-9,
            "output_cost: {}",
            cost.output_cost
        );
        assert!((cost.total - 36.75).abs() < 1e-9, "total: {}", cost.total);
        assert_eq!(cost.price_source, PriceSource::Builtin);
    }

    #[test]
    fn distinguishes_5m_and_1h_cache() {
        let calc = PricingCalculator::new();
        // 500k 5m-cache + 500k 1h-cache for opus-4-6
        let usage = make_usage(0, 0, 1_000_000, 0, 500_000, 500_000);
        let cost = calc.calculate_turn_cost("claude-opus-4-6", &usage);

        // 0.5 MTok * $6.25 = $3.125
        assert!(
            (cost.cache_write_5m_cost - 3.125).abs() < 1e-9,
            "cache_write_5m_cost: {}",
            cost.cache_write_5m_cost
        );
        // 0.5 MTok * $10.0 = $5.0
        assert!(
            (cost.cache_write_1h_cost - 5.0).abs() < 1e-9,
            "cache_write_1h_cost: {}",
            cost.cache_write_1h_cost
        );
        assert!((cost.total - 8.125).abs() < 1e-9, "total: {}", cost.total);
    }

    #[test]
    fn prefix_matching() {
        let calc = PricingCalculator::new();
        let usage = make_usage(1_000_000, 0, 0, 0, 0, 0);
        let cost = calc.calculate_turn_cost("claude-opus-4-5-20251101", &usage);

        // Should match claude-opus-4-5 → base_input = $5.0
        assert!(
            (cost.input_cost - 5.0).abs() < 1e-9,
            "input_cost: {}",
            cost.input_cost
        );
        assert_eq!(cost.price_source, PriceSource::Builtin);
    }

    #[test]
    fn unknown_model_zero() {
        // With no built-in entries at all, an unknown model has no fallback and
        // produces zero cost with `PriceSource::Unknown`. This guards the path
        // taken when `calculate_turn_cost` cannot resolve a price.
        let calc = PricingCalculator {
            prices: HashMap::new(),
            overrides: HashMap::new(),
        };
        let usage = make_usage(1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 0);
        let cost = calc.calculate_turn_cost("gpt-99-turbo", &usage);

        assert!((cost.total - 0.0).abs() < 1e-9, "total: {}", cost.total);
        assert_eq!(cost.price_source, PriceSource::Unknown);
    }

    #[test]
    fn config_override_priority() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "claude-opus-4-6".to_string(),
            ModelPrice {
                base_input: 99.0,
                cache_write_5m: 0.0,
                cache_write_1h: 0.0,
                cache_read: 0.0,
                output: 0.0,
            },
        );

        let calc = PricingCalculator::new().with_overrides(overrides);
        let usage = make_usage(1_000_000, 0, 0, 0, 0, 0);
        let cost = calc.calculate_turn_cost("claude-opus-4-6", &usage);

        assert!(
            (cost.input_cost - 99.0).abs() < 1e-9,
            "input_cost: {}",
            cost.input_cost
        );
        assert_eq!(cost.price_source, PriceSource::Config);
    }

    /// `claude-opus-4-7` must use the same pricing as the opus-4-6 generation
    /// ($5 input / $25 output), not the older opus-4 generation ($15/$75).
    /// Previously opus-4-7 fell through the prefix chain to `claude-opus-4`,
    /// inflating its cost ~3x.
    #[test]
    fn opus_4_7_uses_opus_4_6_pricing() {
        let calc = PricingCalculator::new();
        let usage = make_usage(1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 0);
        let cost = calc.calculate_turn_cost("claude-opus-4-7", &usage);

        // Same total as opus-4-6 with the same usage: 5 + 6.25 + 0.50 + 25 = 36.75
        assert!(
            (cost.input_cost - 5.0).abs() < 1e-9,
            "input_cost: {}",
            cost.input_cost
        );
        assert!(
            (cost.output_cost - 25.0).abs() < 1e-9,
            "output_cost: {}",
            cost.output_cost
        );
        assert!(
            (cost.cache_write_5m_cost - 6.25).abs() < 1e-9,
            "cache_write_5m_cost: {}",
            cost.cache_write_5m_cost
        );
        assert!(
            (cost.cache_read_cost - 0.50).abs() < 1e-9,
            "cache_read_cost: {}",
            cost.cache_read_cost
        );
        assert!((cost.total - 36.75).abs() < 1e-9, "total: {}", cost.total);
        assert_eq!(cost.price_source, PriceSource::Builtin);
    }

    /// `claude-opus-4-8` (with and without a `[1m]` context-window suffix) must
    /// resolve to the opus-4-6/4-7 generation pricing ($5 input / $25 output),
    /// never the older `claude-opus-4` generation ($15/$75). The bracketed
    /// variant previously prefix-matched `claude-opus-4`, ~3x over-pricing.
    #[test]
    fn opus_4_8_uses_opus_generation_pricing_not_opus_4() {
        let calc = PricingCalculator::new();
        let usage = make_usage(1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 0);

        for model in [
            "claude-opus-4-8",
            "claude-opus-4-8[1m]",
            "claude-opus-4-8[200k]",
        ] {
            let cost = calc.calculate_turn_cost(model, &usage);
            // opus-4-6/4-7/4-8 rates: 5 + 6.25 + 0.50 + 25 = 36.75 (NOT 110.25 @ opus-4).
            assert!(
                (cost.input_cost - 5.0).abs() < 1e-9,
                "{model} input_cost: {} (must be opus-gen $5, not opus-4 $15)",
                cost.input_cost
            );
            assert!(
                (cost.output_cost - 25.0).abs() < 1e-9,
                "{model} output_cost: {} (must be opus-gen $25, not opus-4 $75)",
                cost.output_cost
            );
            assert!(
                (cost.total - 36.75).abs() < 1e-9,
                "{model} total: {} (must be 36.75, not opus-4's 110.25)",
                cost.total
            );
            assert_eq!(
                cost.price_source,
                PriceSource::Builtin,
                "{model} must resolve to a builtin entry, not a fallback"
            );
        }
    }

    /// An unknown model name with no prefix overlap with any built-in entry
    /// must fall back to `LATEST_FALLBACK_MODEL` (currently claude-opus-4-8)
    /// with a `PriceSource::Fallback` so the cost is not silently $0.
    ///
    /// Note: we pick "claude-future-x-1" deliberately — names like
    /// `claude-opus-4-999` would be eaten by the `claude-opus-4` prefix.
    #[test]
    fn unknown_model_falls_back_to_latest_with_warning() {
        let calc = PricingCalculator::new();
        let usage = make_usage(1_000_000, 1_000_000, 0, 0, 0, 0);
        let cost = calc.calculate_turn_cost("claude-future-x-1", &usage);

        // Priced at LATEST_FALLBACK_MODEL (opus-4-7) rates: $5 input + $25 output = $30.
        assert!((cost.total - 30.0).abs() < 1e-9, "total: {}", cost.total);
        match cost.price_source {
            PriceSource::Fallback {
                ref requested,
                ref fallback_to,
            } => {
                assert_eq!(requested, "claude-future-x-1");
                assert_eq!(fallback_to, LATEST_FALLBACK_MODEL);
            }
            other => panic!("expected PriceSource::Fallback, got {:?}", other),
        }
    }

    /// Guard against typos: the constant pointed at by `LATEST_FALLBACK_MODEL`
    /// must actually exist in the built-in table; otherwise `get_price` would
    /// silently fall through to `None` and `PriceSource::Unknown`.
    #[test]
    fn fallback_model_must_exist_in_builtin() {
        let calc = PricingCalculator::new();
        assert!(
            calc.prices.contains_key(LATEST_FALLBACK_MODEL),
            "LATEST_FALLBACK_MODEL ({}) must exist in builtin_prices()",
            LATEST_FALLBACK_MODEL
        );
        assert!(calc.latest_builtin_claude().is_some());
    }

    /// `calculate_turn_cost` must propagate the `PriceSource` from `get_price`
    /// onto the `CostBreakdown`, so downstream code can surface fallback warnings.
    #[test]
    fn cost_breakdown_carries_source() {
        let calc = PricingCalculator::new();
        let usage = make_usage(1_000_000, 0, 0, 0, 0, 0);

        let builtin = calc.calculate_turn_cost("claude-opus-4-6", &usage);
        assert_eq!(builtin.price_source, PriceSource::Builtin);

        let fallback = calc.calculate_turn_cost("claude-future-x-1", &usage);
        assert!(matches!(
            fallback.price_source,
            PriceSource::Fallback { .. }
        ));

        let mut overrides = HashMap::new();
        overrides.insert(
            "claude-opus-4-6".to_string(),
            ModelPrice {
                base_input: 1.0,
                cache_write_5m: 0.0,
                cache_write_1h: 0.0,
                cache_read: 0.0,
                output: 0.0,
            },
        );
        let calc_with_override = PricingCalculator::new().with_overrides(overrides);
        let config = calc_with_override.calculate_turn_cost("claude-opus-4-6", &usage);
        assert_eq!(config.price_source, PriceSource::Config);
    }
}
