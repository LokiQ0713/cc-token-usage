/// The Vue frontend template, built from frontend/dist/index.html
/// Rebuild with: cd frontend && npm run build && cp frontend/dist/index.html crates/cc-token-usage/src/output/template.html
const TEMPLATE: &str = include_str!("template.html");

/// Render the new Vue dashboard by injecting real data into the template.
///
/// Escapes dangerous sequences in JSON payload before embedding in `<script>`:
/// - `</` → `<\/` prevents premature `</script>` closure
/// - `<!--` → `<\!--` prevents HTML comment injection
pub fn render_vue_dashboard(json_payload: &str) -> String {
    let safe_payload = json_payload.replace("</", "<\\/").replace("<!--", "<\\!--");
    TEMPLATE.replace("\"__DATA_PLACEHOLDER__\"", &safe_payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The template MUST contain the exact placeholder string we replace.
    /// If a future frontend rebuild renames or removes it, the replace becomes
    /// a no-op and the dashboard ships as a blank page. Catch that at test time.
    #[test]
    fn template_contains_placeholder() {
        assert!(
            TEMPLATE.contains("\"__DATA_PLACEHOLDER__\""),
            "template.html must contain the quoted __DATA_PLACEHOLDER__ token; \
             did the frontend rebuild rename window.__CC_DATA__'s initial value?"
        );
    }

    /// After rendering, the placeholder must be gone and the payload must be
    /// substituted in its place. This is the only end-to-end guarantee that
    /// the template/data wiring works.
    #[test]
    fn render_substitutes_placeholder_with_payload() {
        let payload = r#"{"totalSessions":3,"totalCost":1.23,"_marker":"snapshot_assertion"}"#;
        let html = render_vue_dashboard(payload);

        assert!(
            !html.contains("\"__DATA_PLACEHOLDER__\""),
            "placeholder must be substituted; replace() failed silently"
        );
        assert!(
            html.contains("\"_marker\":\"snapshot_assertion\""),
            "injected payload must appear in rendered HTML"
        );
        assert!(
            html.contains("\"totalSessions\":3"),
            "known camelCase key must survive substitution intact"
        );
    }

    /// `</` and `<!--` inside the payload must be escaped so they cannot
    /// terminate the surrounding `<script>` block or inject HTML comments.
    #[test]
    fn render_escapes_dangerous_html_sequences() {
        let payload = r#"{"evil":"</script><!--xss-->"}"#;
        let html = render_vue_dashboard(payload);

        // The literal `</script>` from the payload must not survive into HTML.
        // We can't assert "no </script>" globally (the real template has its
        // own closing tags), but the payload's `</` must be escaped.
        assert!(
            !html.contains(r#""evil":"</script>"#),
            "raw </script> inside payload must be escaped"
        );
        assert!(
            html.contains(r#""evil":"<\/script><\!--xss-->""#),
            "</ and <!-- must be escaped to <\\/ and <\\!--"
        );
    }
}
