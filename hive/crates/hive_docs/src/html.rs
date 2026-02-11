/// Generate a complete HTML document with the given title and body HTML content.
pub fn generate_html(title: &str, body_html: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; margin: 2rem; line-height: 1.6; color: #333; }}
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background-color: #f4f4f4; font-weight: 600; }}
        tr:nth-child(even) {{ background-color: #fafafa; }}
    </style>
</head>
<body>
{body_html}
</body>
</html>"#,
        title = escape_html(title),
        body_html = body_html,
    )
}

/// Generate an HTML table from headers and rows.
///
/// Cell content is HTML-escaped to prevent injection.
pub fn generate_html_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut html = String::from("<table>\n<thead>\n<tr>\n");

    for header in headers {
        html.push_str(&format!("    <th>{}</th>\n", escape_html(header)));
    }
    html.push_str("</tr>\n</thead>\n<tbody>\n");

    for row in rows {
        html.push_str("<tr>\n");
        for cell in row {
            html.push_str(&format!("    <td>{}</td>\n", escape_html(cell)));
        }
        html.push_str("</tr>\n");
    }

    html.push_str("</tbody>\n</table>");
    html
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_html_structure() {
        let html = generate_html("Test Page", "<p>Hello</p>");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>Test Page</title>"));
        assert!(html.contains("<p>Hello</p>"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn test_generate_html_escapes_title() {
        let html = generate_html("<script>alert('xss')</script>", "body");
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_generate_html_table_basic() {
        let headers = &["Name", "Age"];
        let rows = vec![vec!["Alice".into(), "30".into()]];
        let table = generate_html_table(headers, &rows);
        assert!(table.contains("<th>Name</th>"));
        assert!(table.contains("<th>Age</th>"));
        assert!(table.contains("<td>Alice</td>"));
        assert!(table.contains("<td>30</td>"));
    }

    #[test]
    fn test_generate_html_table_escapes_cells() {
        let headers = &["Data"];
        let rows = vec![vec!["<b>bold</b>".into()]];
        let table = generate_html_table(headers, &rows);
        assert!(table.contains("&lt;b&gt;bold&lt;/b&gt;"));
        assert!(!table.contains("<b>bold</b>"));
    }

    #[test]
    fn test_generate_html_table_empty_rows() {
        let headers = &["Col1", "Col2"];
        let rows: Vec<Vec<String>> = vec![];
        let table = generate_html_table(headers, &rows);
        assert!(table.contains("<th>Col1</th>"));
        assert!(table.contains("<tbody>\n</tbody>"));
    }
}
