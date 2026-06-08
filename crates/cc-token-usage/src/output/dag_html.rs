/// Self-contained HTML template with vis.js DAG renderer.
const DAG_TEMPLATE: &str = include_str!("dag_template.html");

use crate::dag::DagGraph;

/// Render a session DAG as a standalone HTML page.
///
/// The placeholder `__DAG_DATA__` in the template is replaced with the
/// serialized [`DagGraph`] JSON. The data is embedded inside a
/// `<script type="application/json">` block so no JS-string escaping is
/// needed — only `</` → `<\/` to prevent premature `</script>` closure.
pub fn render_dag_html(graph: &DagGraph) -> String {
    let json = serde_json::to_string(graph).expect("DagGraph must be serializable");
    let safe = json.replace("</", "<\\/");
    DAG_TEMPLATE.replace("__DAG_DATA__", &safe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_contains_placeholder() {
        assert!(
            DAG_TEMPLATE.contains("__DAG_DATA__"),
            "dag_template.html must contain the __DAG_DATA__ token"
        );
    }

    #[test]
    fn render_substitutes_placeholder() {
        use crate::dag::DagNode;
        let graph = DagGraph {
            session_id: "test-session".into(),
            nodes: vec![DagNode {
                id: "u1".into(),
                label: "test".into(),
                full_label: "tooltip".into(),
                level: 0,
                entry_type: "user".into(),
                agent_id: None,
                is_sidechain: false,
                tokens: Some(100),
                cost: None,
                timestamp: "2026-01-01T00:00:00Z".into(),
            }],
            edges: vec![],
        };

        let html = render_dag_html(&graph);
        assert!(!html.contains("__DAG_DATA__"), "placeholder must be gone");
        assert!(html.contains("test-session"), "payload session_id must be present");
    }
}
