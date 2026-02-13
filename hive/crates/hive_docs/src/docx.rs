use anyhow::Result;
use docx_rs::*;
use std::io::Cursor;

/// Generate a DOCX document with a title and a series of sections.
///
/// The title is rendered as a large, bold heading. Each section gets a bold
/// heading followed by its body text (split on newlines into separate paragraphs).
pub fn generate_docx_document(title: &str, sections: &[(&str, &str)]) -> Result<Vec<u8>> {
    let mut docx = Docx::new();

    // Title paragraph -- large bold text
    let title_run = Run::new().add_text(title).bold().size(48); // size is in half-points, so 48 = 24pt
    docx = docx.add_paragraph(Paragraph::new().add_run(title_run));

    // Spacer paragraph
    docx = docx.add_paragraph(Paragraph::new());

    for (heading, body) in sections {
        // Section heading -- bold, medium size
        let heading_run = Run::new().add_text(*heading).bold().size(32); // 16pt
        docx = docx.add_paragraph(Paragraph::new().add_run(heading_run));

        // Body text -- split by newlines into separate paragraphs
        for line in body.lines() {
            let body_run = Run::new().add_text(line).size(22); // 11pt
            docx = docx.add_paragraph(Paragraph::new().add_run(body_run));
        }

        // Spacer after each section
        docx = docx.add_paragraph(Paragraph::new());
    }

    let mut buf = Cursor::new(Vec::new());
    docx.build()
        .pack(&mut buf)
        .map_err(|e| anyhow::anyhow!("Failed to pack DOCX: {}", e))?;

    Ok(buf.into_inner())
}

/// Generate a DOCX document containing a table with headers and rows.
///
/// Headers are rendered in bold. The table spans the full width with evenly
/// distributed columns.
pub fn generate_docx_table(title: &str, headers: &[&str], rows: &[Vec<String>]) -> Result<Vec<u8>> {
    let mut docx = Docx::new();

    // Title
    let title_run = Run::new().add_text(title).bold().size(36); // 18pt
    docx = docx.add_paragraph(Paragraph::new().add_run(title_run));
    docx = docx.add_paragraph(Paragraph::new());

    // Build table rows
    let mut table_rows = Vec::new();

    // Header row
    let header_cells: Vec<TableCell> = headers
        .iter()
        .map(|h| {
            let run = Run::new().add_text(*h).bold().size(22);
            TableCell::new().add_paragraph(Paragraph::new().add_run(run))
        })
        .collect();
    table_rows.push(TableRow::new(header_cells));

    // Data rows
    for row in rows {
        let cells: Vec<TableCell> = row
            .iter()
            .map(|cell_text| {
                let run = Run::new().add_text(cell_text).size(22);
                TableCell::new().add_paragraph(Paragraph::new().add_run(run))
            })
            .collect();
        table_rows.push(TableRow::new(cells));
    }

    let table = Table::new(table_rows);
    docx = docx.add_table(table);

    let mut buf = Cursor::new(Vec::new());
    docx.build()
        .pack(&mut buf)
        .map_err(|e| anyhow::anyhow!("Failed to pack DOCX: {}", e))?;

    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_docx_document_basic() {
        let sections = vec![
            ("Introduction", "This is the intro paragraph."),
            ("Conclusion", "Final thoughts here."),
        ];
        let bytes = generate_docx_document("Test Report", &sections).unwrap();
        // DOCX is a zip file -- starts with PK magic bytes
        assert!(bytes.len() > 100);
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn test_generate_docx_document_empty_sections() {
        let bytes = generate_docx_document("Empty Doc", &[]).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
        assert!(bytes.len() > 50);
    }

    #[test]
    fn test_generate_docx_document_multiline_body() {
        let sections = vec![("Content", "First line\nSecond line\nThird line")];
        let bytes = generate_docx_document("Multiline", &sections).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn test_generate_docx_document_special_characters() {
        let sections = vec![
            ("Symbols", "Price: $100 & 10% <off>"),
            ("Quotes", "She said \"hello\" and he said 'hi'"),
        ];
        let bytes = generate_docx_document("Special Chars", &sections).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
        assert!(bytes.len() > 100);
    }

    #[test]
    fn test_generate_docx_table_basic() {
        let headers = &["Name", "Age", "City"];
        let rows = vec![
            vec!["Alice".into(), "30".into(), "New York".into()],
            vec!["Bob".into(), "25".into(), "London".into()],
        ];
        let bytes = generate_docx_table("People", headers, &rows).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
        assert!(bytes.len() > 200);
    }

    #[test]
    fn test_generate_docx_table_empty_rows() {
        let headers = &["Col1", "Col2"];
        let rows: Vec<Vec<String>> = vec![];
        let bytes = generate_docx_table("Empty Table", headers, &rows).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn test_generate_docx_table_single_cell() {
        let headers = &["Value"];
        let rows = vec![vec!["42".into()]];
        let bytes = generate_docx_table("Single", headers, &rows).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn test_generate_docx_table_many_columns() {
        let headers = &["A", "B", "C", "D", "E"];
        let rows = vec![vec![
            "1".into(),
            "2".into(),
            "3".into(),
            "4".into(),
            "5".into(),
        ]];
        let bytes = generate_docx_table("Wide Table", headers, &rows).unwrap();
        assert_eq!(&bytes[0..2], b"PK");
    }
}
