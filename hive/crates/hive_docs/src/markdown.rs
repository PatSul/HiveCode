/// Generate a Markdown table from headers and rows.
///
/// Pipes in cell content are escaped to prevent breaking the table structure.
pub fn generate_markdown_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    if headers.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();

    // Header row
    let header_cells: Vec<String> = headers.iter().map(|h| escape_pipe(h)).collect();
    lines.push(format!("| {} |", header_cells.join(" | ")));

    // Separator row
    let separators: Vec<&str> = headers.iter().map(|_| "---").collect();
    lines.push(format!("| {} |", separators.join(" | ")));

    // Data rows
    for row in rows {
        let cells: Vec<String> = row.iter().map(|c| escape_pipe(c)).collect();
        lines.push(format!("| {} |", cells.join(" | ")));
    }

    lines.join("\n")
}

/// Generate a Markdown document with a title and a series of sections.
///
/// Each section gets an `## heading` followed by its body text.
pub fn generate_markdown_document(title: &str, sections: &[(&str, &str)]) -> String {
    let mut parts = Vec::new();

    parts.push(format!("# {title}"));

    for (heading, body) in sections {
        parts.push(format!("## {heading}"));
        parts.push((*body).to_string());
    }

    parts.join("\n\n")
}

fn escape_pipe(s: &str) -> String {
    s.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_markdown_table_basic() {
        let headers = &["Name", "Score"];
        let rows = vec![
            vec!["Alice".into(), "95".into()],
            vec!["Bob".into(), "87".into()],
        ];
        let table = generate_markdown_table(headers, &rows);
        let lines: Vec<&str> = table.lines().collect();

        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "| Name | Score |");
        assert_eq!(lines[1], "| --- | --- |");
        assert_eq!(lines[2], "| Alice | 95 |");
        assert_eq!(lines[3], "| Bob | 87 |");
    }

    #[test]
    fn test_generate_markdown_table_escapes_pipes() {
        let headers = &["Data"];
        let rows = vec![vec!["a|b".into()]];
        let table = generate_markdown_table(headers, &rows);
        assert!(table.contains("a\\|b"));
    }

    #[test]
    fn test_generate_markdown_table_empty_headers() {
        let table = generate_markdown_table(&[], &[]);
        assert_eq!(table, "");
    }

    #[test]
    fn test_generate_markdown_document_basic() {
        let doc = generate_markdown_document(
            "My Report",
            &[
                ("Introduction", "This is the intro."),
                ("Results", "Here are the results."),
            ],
        );
        assert!(doc.starts_with("# My Report"));
        assert!(doc.contains("## Introduction"));
        assert!(doc.contains("This is the intro."));
        assert!(doc.contains("## Results"));
        assert!(doc.contains("Here are the results."));
    }

    #[test]
    fn test_generate_markdown_document_no_sections() {
        let doc = generate_markdown_document("Title Only", &[]);
        assert_eq!(doc, "# Title Only");
    }
}
