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
            });

        acc.session_count += 1;

        for turn in &session.turns {
            acc.tokens.add_usage(&turn.usage);
            acc.total_turns += 1;
            let cost = calc.calculate_turn_cost(&turn.model, &turn.usage);
            acc.cost += cost.total;
        }

        for turn in &session.agent_turns {
            acc.tokens.add_usage(&turn.usage);
            acc.total_turns += 1;
            acc.agent_turns += 1;
            let cost = calc.calculate_turn_cost(&turn.model, &turn.usage);
            acc.cost += cost.total;
        }
    }

    let mut projects: Vec<ProjectSummary> = project_map
        .into_values()
        .map(|acc| ProjectSummary {
            display_name: project_display_name(&acc.name),
            name: acc.name,
            session_count: acc.session_count,
            total_turns: acc.total_turns,
            agent_turns: acc.agent_turns,
            tokens: acc.tokens,
            cost: acc.cost,
        })
        .collect();

    // Sort by cost descending
    projects.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));

    // Take top_n (0 means no limit)
    if top_n > 0 {
        projects.truncate(top_n);
    }

    ProjectResult { projects }
}

/// Convert internal project path to a human-readable display name.
///
/// macOS/Linux: `-Users-testuser-cc-web3` -> `~/cc/web3`
/// Windows:     `C--Users-JohnDoe-dev-myapp` -> `~/dev/myapp`
pub fn project_display_name(name: &str) -> String {
    // Windows pattern: `C--Users-<username>-<rest>` (drive letter prefix)
    // Match: single letter followed by `--Users-`
    if name.len() > 3 && name.as_bytes()[1] == b'-' && name[2..].starts_with("-Users-") {
        let rest = &name[9..]; // skip "X--Users-"
        if let Some(after_user) = rest.find('-') {
            let path_part = &rest[after_user..];
            let display = path_part.replace('-', "/");
            return format!("~{display}");
        }
    }

    // macOS/Linux pattern: `-Users-<username>-<rest>`
    if let Some(rest) = name.strip_prefix("-Users-") {
        if let Some(after_user) = rest.find('-') {
            let path_part = &rest[after_user..];
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_display_name() {
        // macOS/Linux paths
        assert_eq!(
            project_display_name("-Users-testuser-cc-web3"),
            "~/cc/web3"
        );
        assert_eq!(
            project_display_name("-Users-alice-projects-my-app"),
            "~/projects/my/app"
        );
        assert_eq!(project_display_name("simple-project"), "simple-project");
        assert_eq!(project_display_name("-Users-bob"), "-Users-bob");

        // Windows paths (drive letter prefix)
        assert_eq!(
            project_display_name("C--Users-JohnDoe-dev-myapp"),
            "~/dev/myapp"
        );
        assert_eq!(
            project_display_name("D--Users-Alice-Documents-projects-web"),
            "~/Documents/projects/web"
        );
        assert_eq!(
            project_display_name("C--Users-Bob"),
            "C--Users-Bob" // no path after username
        );
    }
}
