//! PDF document generation.
//!
//! Generates minimal but valid PDF files using raw PDF format construction.
//! Supports document generation with titled sections and table generation.
//! Uses built-in Helvetica font — no external font files required.

use anyhow::Result;

/// Generate a PDF document with a title and a series of sections.
///
/// Each section has a heading (bold) and body text.
pub fn generate_pdf_document(title: &str, sections: &[(&str, &str)]) -> Result<Vec<u8>> {
    let mut builder = PdfBuilder::new();

    // Build content stream
    let mut content = String::new();
    let mut y = 770.0_f64; // start near top of A4 (842pt height)

    // Title
    content.push_str("BT\n");
    content.push_str("/F1 22 Tf\n");
    content.push_str(&format!("72 {y:.0} Td\n"));
    content.push_str(&format!("({}) Tj\n", pdf_escape(title)));
    content.push_str("ET\n");
    y -= 36.0;

    // Sections
    for (heading, body) in sections {
        y -= 16.0;

        // Heading
        content.push_str("BT\n");
        content.push_str("/F1 14 Tf\n");
        content.push_str(&format!("72 {y:.0} Td\n"));
        content.push_str(&format!("({}) Tj\n", pdf_escape(heading)));
        content.push_str("ET\n");
        y -= 20.0;

        // Body lines
        content.push_str("BT\n");
        content.push_str("/F2 11 Tf\n");
        content.push_str(&format!("72 {y:.0} Td\n"));
        content.push_str("0 -15.4 Td\n"); // set leading for TL

        for line in body.lines() {
            content.push_str(&format!("({}) Tj\n", pdf_escape(line)));
            content.push_str("0 -15.4 Td\n");
            y -= 15.4;
        }
        content.push_str("ET\n");
    }

    builder.set_content(&content);
    Ok(builder.build(title))
}

/// Generate a PDF document containing a table with headers and rows.
///
/// Headers are rendered in bold; columns are evenly distributed.
pub fn generate_pdf_table(title: &str, headers: &[&str], rows: &[Vec<String>]) -> Result<Vec<u8>> {
    let num_cols = headers.len().max(1);
    let usable_width = 468.0_f64; // 612 - 72*2 margins
    let col_width = usable_width / num_cols as f64;

    let mut content = String::new();
    let mut y = 770.0_f64;

    // Title
    content.push_str("BT\n");
    content.push_str("/F1 18 Tf\n");
    content.push_str(&format!("72 {y:.0} Td\n"));
    content.push_str(&format!("({}) Tj\n", pdf_escape(title)));
    content.push_str("ET\n");
    y -= 30.0;

    let row_height = 18.0_f64;

    // Header background (light gray)
    content.push_str("0.9 0.9 0.9 rg\n"); // fill color
    content.push_str(&format!(
        "72 {:.0} {usable_width:.0} {row_height:.0} re f\n",
        y - row_height
    ));

    // Header text
    content.push_str("0 0 0 rg\n"); // black text
    for (i, header) in headers.iter().enumerate() {
        let x = 72.0 + i as f64 * col_width + 4.0;
        content.push_str("BT\n");
        content.push_str("/F1 10 Tf\n");
        content.push_str(&format!("{x:.0} {:.0} Td\n", y - row_height + 5.0));
        content.push_str(&format!("({}) Tj\n", pdf_escape(header)));
        content.push_str("ET\n");
    }
    y -= row_height;

    // Data rows
    for (row_idx, row) in rows.iter().enumerate() {
        // Alternating background
        if row_idx % 2 == 0 {
            content.push_str("0.96 0.96 0.96 rg\n");
            content.push_str(&format!(
                "72 {:.0} {usable_width:.0} {row_height:.0} re f\n",
                y - row_height
            ));
        }

        content.push_str("0 0 0 rg\n");
        for (col_idx, cell) in row.iter().enumerate() {
            let x = 72.0 + col_idx as f64 * col_width + 4.0;
            content.push_str("BT\n");
            content.push_str("/F2 10 Tf\n");
            content.push_str(&format!("{x:.0} {:.0} Td\n", y - row_height + 5.0));
            content.push_str(&format!("({}) Tj\n", pdf_escape(cell)));
            content.push_str("ET\n");
        }
        y -= row_height;
    }

    // Table border
    content.push_str("0.6 0.6 0.6 RG\n"); // stroke color
    content.push_str("0.5 w\n"); // line width
    let table_top = 770.0 - 30.0;
    let table_height = table_top - y;
    content.push_str(&format!(
        "72 {y:.0} {usable_width:.0} {table_height:.0} re S\n"
    ));

    let mut builder = PdfBuilder::new();
    builder.set_content(&content);
    Ok(builder.build(title))
}

/// Escape special characters for PDF string literals.
fn pdf_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// Minimal PDF file builder. Constructs valid PDF 1.4 files.
struct PdfBuilder {
    content: String,
}

impl PdfBuilder {
    fn new() -> Self {
        Self {
            content: String::new(),
        }
    }

    fn set_content(&mut self, content: &str) {
        self.content = content.to_string();
    }

    /// Build the complete PDF file as bytes.
    fn build(&self, title: &str) -> Vec<u8> {
        let mut pdf = String::new();
        let mut offsets: Vec<usize> = Vec::new();

        // Header
        pdf.push_str("%PDF-1.4\n");

        // Obj 1: Catalog
        offsets.push(pdf.len());
        pdf.push_str("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        // Obj 2: Pages
        offsets.push(pdf.len());
        pdf.push_str("2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

        // Obj 3: Page
        offsets.push(pdf.len());
        pdf.push_str("3 0 obj\n<< /Type /Page /Parent 2 0 R ");
        pdf.push_str("/MediaBox [0 0 612 842] "); // A4 in points
        pdf.push_str("/Contents 4 0 R /Resources << /Font << ");
        pdf.push_str("/F1 5 0 R /F2 6 0 R >> >> >>\nendobj\n");

        // Obj 4: Content stream
        offsets.push(pdf.len());
        let stream = &self.content;
        pdf.push_str(&format!(
            "4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
            stream.len(),
            stream
        ));

        // Obj 5: Font (Helvetica-Bold)
        offsets.push(pdf.len());
        pdf.push_str(
            "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>\nendobj\n",
        );

        // Obj 6: Font (Helvetica)
        offsets.push(pdf.len());
        pdf.push_str(
            "6 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
        );

        // Obj 7: Info (title)
        offsets.push(pdf.len());
        pdf.push_str(&format!(
            "7 0 obj\n<< /Title ({}) /Producer (Hive) >>\nendobj\n",
            pdf_escape(title)
        ));

        // Cross-reference table
        let xref_offset = pdf.len();
        let num_objects = offsets.len() + 1; // +1 for free entry
        pdf.push_str(&format!("xref\n0 {num_objects}\n"));
        pdf.push_str("0000000000 65535 f \n");
        for offset in &offsets {
            pdf.push_str(&format!("{:010} 00000 n \n", offset));
        }

        // Trailer
        pdf.push_str(&format!(
            "trailer\n<< /Size {num_objects} /Root 1 0 R /Info 7 0 R >>\n"
        ));
        pdf.push_str(&format!("startxref\n{xref_offset}\n%%EOF\n"));

        pdf.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_pdf_document_basic() {
        let sections = vec![
            ("Introduction", "This is the introduction."),
            ("Conclusion", "This wraps it up."),
        ];
        let bytes = generate_pdf_document("Test Report", &sections).unwrap();
        assert!(bytes.len() > 100);
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_generate_pdf_document_empty_sections() {
        let bytes = generate_pdf_document("Empty Doc", &[]).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 50);
    }

    #[test]
    fn test_generate_pdf_document_multiline_body() {
        let sections = vec![("Notes", "Line one\nLine two\nLine three")];
        let bytes = generate_pdf_document("Multiline", &sections).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_generate_pdf_table_basic() {
        let headers = &["Name", "Age", "City"];
        let rows = vec![
            vec!["Alice".into(), "30".into(), "New York".into()],
            vec!["Bob".into(), "25".into(), "London".into()],
        ];
        let bytes = generate_pdf_table("People", headers, &rows).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 200);
    }

    #[test]
    fn test_generate_pdf_table_empty_rows() {
        let headers = &["Col1", "Col2"];
        let rows: Vec<Vec<String>> = vec![];
        let bytes = generate_pdf_table("Empty Table", headers, &rows).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_generate_pdf_table_single_cell() {
        let headers = &["Value"];
        let rows = vec![vec!["42".into()]];
        let bytes = generate_pdf_table("Single Cell", headers, &rows).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_generate_pdf_special_characters() {
        let sections = vec![
            ("Symbols & Signs", "Price: $100 @ 10% off (sale)"),
        ];
        let bytes = generate_pdf_document("Special Chars", &sections).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        // Parentheses should be escaped
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("\\(sale\\)"));
    }

    #[test]
    fn test_pdf_escape() {
        assert_eq!(pdf_escape("hello"), "hello");
        assert_eq!(pdf_escape("(test)"), "\\(test\\)");
        assert_eq!(pdf_escape("a\\b"), "a\\\\b");
    }
}
