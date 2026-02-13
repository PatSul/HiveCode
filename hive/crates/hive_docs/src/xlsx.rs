use anyhow::{Context, Result};
use rust_xlsxwriter::{Format, Workbook};

/// Generate an XLSX file from headers and rows.
///
/// Returns the raw bytes of the xlsx file (can be written to disk or sent as download).
pub fn generate_xlsx(headers: &[&str], rows: &[Vec<String>]) -> Result<Vec<u8>> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    let header_format = Format::new().set_bold();

    // Write header row
    for (col, header) in headers.iter().enumerate() {
        worksheet
            .write_string_with_format(0, col as u16, *header, &header_format)
            .with_context(|| format!("Failed to write header at column {col}"))?;
    }

    // Write data rows
    for (row_idx, row) in rows.iter().enumerate() {
        let excel_row = (row_idx + 1) as u32;
        for (col_idx, cell) in row.iter().enumerate() {
            // Try to parse as number, otherwise write as string
            if let Ok(num) = cell.parse::<f64>() {
                worksheet
                    .write_number(excel_row, col_idx as u16, num)
                    .with_context(|| {
                        format!("Failed to write number at ({excel_row}, {col_idx})")
                    })?;
            } else {
                worksheet
                    .write_string(excel_row, col_idx as u16, cell)
                    .with_context(|| {
                        format!("Failed to write string at ({excel_row}, {col_idx})")
                    })?;
            }
        }
    }

    // Auto-fit columns for readability
    worksheet.autofit();

    let bytes = workbook
        .save_to_buffer()
        .context("Failed to save workbook to buffer")?;

    Ok(bytes)
}

/// Generate an XLSX file with multiple named sheets.
pub fn generate_xlsx_multi_sheet(sheets: &[(&str, &[&str], &[Vec<String>])]) -> Result<Vec<u8>> {
    let mut workbook = Workbook::new();
    let header_format = Format::new().set_bold();

    for (sheet_name, headers, rows) in sheets {
        let worksheet = workbook.add_worksheet();
        worksheet
            .set_name(*sheet_name)
            .with_context(|| format!("Failed to set sheet name: {sheet_name}"))?;

        for (col, header) in headers.iter().enumerate() {
            worksheet
                .write_string_with_format(0, col as u16, *header, &header_format)
                .with_context(|| format!("Failed to write header at column {col}"))?;
        }

        for (row_idx, row) in rows.iter().enumerate() {
            let excel_row = (row_idx + 1) as u32;
            for (col_idx, cell) in row.iter().enumerate() {
                if let Ok(num) = cell.parse::<f64>() {
                    worksheet.write_number(excel_row, col_idx as u16, num)?;
                } else {
                    worksheet.write_string(excel_row, col_idx as u16, cell)?;
                }
            }
        }

        worksheet.autofit();
    }

    let bytes = workbook
        .save_to_buffer()
        .context("Failed to save workbook to buffer")?;

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_xlsx_basic() {
        let headers = &["Name", "Age", "City"];
        let rows = vec![
            vec!["Alice".into(), "30".into(), "New York".into()],
            vec!["Bob".into(), "25".into(), "London".into()],
        ];
        let bytes = generate_xlsx(headers, &rows).unwrap();
        // XLSX files start with PK (zip format)
        assert!(bytes.len() > 100);
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn test_generate_xlsx_empty_rows() {
        let headers = &["Col1", "Col2"];
        let rows: Vec<Vec<String>> = vec![];
        let bytes = generate_xlsx(headers, &rows).unwrap();
        assert!(bytes.len() > 100);
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn test_generate_xlsx_numeric_cells() {
        let headers = &["Item", "Price"];
        let rows = vec![
            vec!["Widget".into(), "9.99".into()],
            vec!["Gadget".into(), "19.50".into()],
        ];
        let bytes = generate_xlsx(headers, &rows).unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_generate_xlsx_special_characters() {
        let headers = &["Data"];
        let rows = vec![
            vec!["Hello, \"World\"".into()],
            vec!["Line1\nLine2".into()],
            vec!["Tab\there".into()],
        ];
        let bytes = generate_xlsx(headers, &rows).unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_generate_xlsx_multi_sheet() {
        let sheet1_headers = &["Name", "Score"];
        let sheet1_rows = vec![vec!["Alice".into(), "95".into()]];

        let sheet2_headers = &["Product", "Price"];
        let sheet2_rows = vec![vec!["Widget".into(), "9.99".into()]];

        let sheets: Vec<(&str, &[&str], &[Vec<String>])> = vec![
            ("Scores", sheet1_headers, &sheet1_rows),
            ("Products", sheet2_headers, &sheet2_rows),
        ];

        let bytes = generate_xlsx_multi_sheet(&sheets).unwrap();
        assert!(bytes.len() > 100);
        assert_eq!(&bytes[0..2], b"PK");
    }

    #[test]
    fn test_generate_xlsx_large_dataset() {
        let headers = &["ID", "Value"];
        let rows: Vec<Vec<String>> = (0..1000)
            .map(|i| vec![i.to_string(), format!("{:.2}", i as f64 * 1.5)])
            .collect();
        let bytes = generate_xlsx(headers, &rows).unwrap();
        assert!(bytes.len() > 1000);
    }

    #[test]
    fn test_generate_xlsx_single_cell() {
        let headers = &["X"];
        let rows = vec![vec!["value".into()]];
        let bytes = generate_xlsx(headers, &rows).unwrap();
        assert!(!bytes.is_empty());
    }
}
