use std::collections::HashMap;

use crate::data::models::SessionData;
use crate::pricing::calculator::PricingCalculator;

use super::{AggregatedTokens, ProjectResult, ProjectSummary};

pub fn analyze_projects(
    sessions: &[SessionData],
    calc: &PricingCalculator,
    top_n: usize,
) -> ProjectResult {
    let mut project_map: HashMap<String, ProjectAccumulator> = HashMap::new();

    for session in sessions {
        let project_name = session
            .project
            .clone()
            .unwrap_or_else(|| "(unknown)".to_string());

        let acc = project_map
            .entry(project_name.clone())
            .or_insert_with(|| ProjectAccumulator {
                name: project_name,
                session_count: 0,
                total_turns: 0,
                agent_turns: 0,
                tokens: AggregatedTokens::default(),
                cost: 0.0,
                model_counts: HashMap::new(),
            });

        acc.session_count += 1;

        for turn in session.all_responses() {
            acc.tokens.add_usage(&turn.usage);
            acc.total_turns += 1;
            if turn.is_agent {
                acc.agent_turns += 1;
            }
            *acc.model_counts.entry(turn.model.clone()).or_insert(0) += 1;
            let cost = calc.calculate_turn_cost(&turn.model, &turn.usage);
            acc.cost += cost.total;
        }
    }

    let mut projects: Vec<ProjectSummary> = project_map
        .into_values()
        .map(|acc| {
            let primary_model = acc
                .model_counts
                .into_iter()
                .max_by_key(|(_, c)| *c)
                .map(|(m, _)| m)
                .unwrap_or_default();
            ProjectSummary {
                display_name: project_display_name(&acc.name),
                name: acc.name,
                session_count: acc.session_count,
                total_turns: acc.total_turns,
                agent_turns: acc.agent_turns,
                tokens: acc.tokens,
                cost: acc.cost,
                primary_model,
            }
        })
        .collect();

    // Sort by cost descending
    projects.sort_by(|a, b| {
        b.cost
            .partial_cmp(&a.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Take top_n (0 means no limit)
    if top_n > 0 {
        projects.truncate(top_n);
    }

    ProjectResult { projects }
}

/// Convert internal project path to a human-readable display name.
///
/// `-Users-testuser-cc-web3` -> `~/cc/web3`
pub fn project_display_name(name: &str) -> String {
    // Pattern: -Users-<username>-<rest>
    // Try to find the "-Users-" prefix and convert
    if let Some(rest) = name.strip_prefix("-Users-") {
        // Skip the username segment (everything up to the next '-')
        if let Some(after_user) = rest.find('-') {
            let path_part = &rest[after_user..];
            // Replace '-' with '/'
            let display = path_part.replace('-', "/");
            return format!("~{display}");
        }
    }

    name.to_string()
}

struct ProjectAccumulator {
    name: String,
    session_count: usize,
    total_turns: usize,
    agent_turns: usize,
    tokens: AggregatedTokens,
    cost: f64,
    model_counts: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::{
        DataQuality, SessionData, SessionMetadata, TokenUsage, ValidatedTurn,
    };
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_project_display_name() {
        assert_eq!(project_display_name("-Users-testuser-cc-web3"), "~/cc/web3");
        assert_eq!(
            project_display_name("-Users-alice-projects-my-app"),
            "~/projects/my/app"
        );
        assert_eq!(project_display_name("simple-project"), "simple-project");
        assert_eq!(project_display_name("-Users-bob"), "-Users-bob");
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn turn(model: &str, input: u64, output: u64) -> ValidatedTurn {
        ValidatedTurn {
            uuid: format!("u-{model}-{input}-{output}"),
            request_id: None,
            timestamp: Utc.with_ymd_and_hms(2025, 5, 1, 12, 0, 0).unwrap(),
            model: model.into(),
            usage: TokenUsage {
                input_tokens: Some(input),
                output_tokens: Some(output),
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                cache_creation: None,
                server_tool_use: None,
                service_tier: None,
                speed: None,
                inference_geo: None,
            },
            stop_reason: Some("end_turn".into()),
            content_types: vec!["text".into()],
            is_agent: false,
            agent_id: None,
            user_text: None,
            assistant_text: None,
            tool_names: vec![],
            service_tier: None,
            speed: None,
            inference_geo: None,
            tool_error_count: 0,
            git_branch: None,
            attribution_plugin: None,
            attribution_skill: None,
        }
    }

    fn session(id: &str, project: Option<&str>, turns: Vec<ValidatedTurn>) -> SessionData {
        SessionData {
            session_id: id.into(),
            project: project.map(|s| s.into()),
            turns,
            subagents: vec![],
            plugins: vec![],
            skills: vec![],
            hooks: vec![],
            first_timestamp: Some(Utc.with_ymd_and_hms(2025, 5, 1, 12, 0, 0).unwrap()),
            last_timestamp: Some(Utc.with_ymd_and_hms(2025, 5, 1, 13, 0, 0).unwrap()),
            version: None,
            quality: DataQuality::default(),
            metadata: SessionMetadata::default(),
            is_orphan: false,
        }
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    /// No sessions at all → empty projects vec. Must not panic; must not
    /// emit an "(unknown)" placeholder row.
    #[test]
    fn empty_sessions_produce_empty_projects() {
        let calc = PricingCalculator::new();
        let result = analyze_projects(&[], &calc, 10);
        assert!(result.projects.is_empty());
    }

    /// Two projects with different costs must rank descending by cost.
    /// `-Users-dev-cheap` has 1 cheap turn; `-Users-dev-expensive` has 1
    /// expensive turn — the expensive one comes first.
    #[test]
    fn projects_sorted_by_cost_desc() {
        let calc = PricingCalculator::new();
        let cheap = session(
            "s-cheap",
            Some("-Users-dev-cheap"),
            vec![turn("claude-opus-4-6", 100, 100)],
        );
        let expensive = session(
            "s-expensive",
            Some("-Users-dev-expensive"),
            vec![turn("claude-opus-4-6", 1_000_000, 1_000_000)],
        );
        let result = analyze_projects(&[cheap, expensive], &calc, 10);

        assert_eq!(result.projects.len(), 2);
        assert!(
            result.projects[0].cost > result.projects[1].cost,
            "first project must be the more expensive one"
        );
        assert!(result.projects[0].name.contains("expensive"));
    }

    /// `top_n=N` truncates after sorting. With 3 projects and `top_n=2`,
    /// only the top 2 by cost remain. `top_n=0` means "no limit."
    #[test]
    fn top_n_truncation_respects_zero_as_no_limit() {
        let calc = PricingCalculator::new();
        let sessions: Vec<_> = (0..3)
            .map(|i| {
                session(
                    &format!("s{i}"),
                    Some(&format!("-Users-dev-proj{i}")),
                    vec![turn("claude-opus-4-6", 1000 * (i as u64 + 1), 1000)],
                )
            })
            .collect();

        let top2 = analyze_projects(&sessions, &calc, 2);
        assert_eq!(top2.projects.len(), 2, "top_n=2 must truncate to 2");

        let all = analyze_projects(&sessions, &calc, 0);
        assert_eq!(all.projects.len(), 3, "top_n=0 must keep all 3");
    }

    /// Session with `project = None` falls into the "(unknown)" bucket
    /// instead of being silently dropped. Important for orphan sessions.
    #[test]
    fn missing_project_falls_into_unknown_bucket() {
        let calc = PricingCalculator::new();
        let orphan = session("s-orphan", None, vec![turn("claude-opus-4-6", 100, 100)]);
        let result = analyze_projects(&[orphan], &calc, 10);

        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].name, "(unknown)");
    }

    /// `primary_model` is the model with the highest *turn count* in the
    /// project, not the most expensive one. A project with 3 sonnet turns
    /// and 1 opus turn reports sonnet as primary even if opus cost more.
    #[test]
    fn primary_model_picks_most_common_by_turn_count() {
        let calc = PricingCalculator::new();
        let s = session(
            "s-mix",
            Some("-Users-dev-mix"),
            vec![
                turn("claude-sonnet-4-5", 100, 100),
                turn("claude-sonnet-4-5", 100, 100),
                turn("claude-sonnet-4-5", 100, 100),
                turn("claude-opus-4-6", 1_000_000, 1_000_000), // expensive but solo
            ],
        );
        let result = analyze_projects(&[s], &calc, 10);

        assert_eq!(result.projects.len(), 1);
        assert_eq!(
            result.projects[0].primary_model, "claude-sonnet-4-5",
            "primary_model is most-frequent, not most-expensive"
        );
    }
}
