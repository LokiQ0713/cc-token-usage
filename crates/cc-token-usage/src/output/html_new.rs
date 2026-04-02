/// The Vue frontend template, built from frontend/dist/index.html
/// Rebuild with: cd frontend && npm run build
const TEMPLATE: &str = include_str!("../../../../frontend/dist/index.html");

/// Render the new Vue dashboard by injecting real data into the template.
pub fn render_vue_dashboard(json_payload: &str) -> String {
    TEMPLATE.replace("\"__DATA_PLACEHOLDER__\"", json_payload)
}
