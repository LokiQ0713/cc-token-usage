/// The Vue frontend template, built from frontend/dist/index.html
/// Rebuild with: cd frontend && npm run build
const TEMPLATE: &str = include_str!("../../../../frontend/dist/index.html");

/// Render the new Vue dashboard by injecting real data into the template.
///
/// Escapes dangerous sequences in JSON payload before embedding in `<script>`:
/// - `</` → `<\/` prevents premature `</script>` closure
/// - `<!--` → `<\!--` prevents HTML comment injection
pub fn render_vue_dashboard(json_payload: &str) -> String {
    let safe_payload = json_payload.replace("</", "<\\/").replace("<!--", "<\\!--");
    TEMPLATE.replace("\"__DATA_PLACEHOLDER__\"", &safe_payload)
}
