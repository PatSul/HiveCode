use anyhow::{Context, Result};

/// Generate a CSV string from headers and rows.
///
/// Each field is properly quoted/escaped by the `csv` crate.
pub fn generate_csv(headers: &[&str], rows: &[Vec<String>]) -> Result<String> {
    generate_delimited(headers, rows, b',')
}

/// Generate a TSV (tab-separated values) string from headers and rows.
pub fn generate_tsv(headers: &[&str], rows: &[Vec<String>]) -> Result<String> {
    generate_delimited(headers, rows, b'\t')
}

/// Parse a CSV string into headers and rows.
///
/// The first record is treated as the header row.
pub fn parse_csv(input: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(input.as_bytes());

    let headers: Vec<String> = reader
        .headers()
        .context("Failed to read CSV headers")?
        .iter()
        .map(String::from)
        .collect();

    let mut rows = Vec::new();
    for result in reader.records() {
        let record = result.context("Failed to read CSV record")?;
        let row: Vec<String> = record.iter().map(String::from).collect();
        rows.push(row);
    }

    Ok((headers, rows))
}

fn generate_delimited(headers: &[&str], rows: &[Vec<String>], delimiter: u8) -> Result<String> {
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());

    writer
        .write_record(headers)
        .context("Failed to write header record")?;

    for row in rows {
        writer
            .write_record(row)
            .context("Failed to write data record")?;
    }

    let bytes = writer.into_inner().context("Failed to flush CSV writer")?;

    String::from_utf8(bytes).context("CSV output contained invalid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_csv_basic() {
        let headers = &["Name", "Age", "City"];
        let rows = vec![
            vec!["Alice".into(), "30".into(), "New York".into()],
            vec!["Bob".into(), "25".into(), "London".into()],
        ];
        let result = generate_csv(headers, &rows).unwrap();
        assert!(result.contains("Name,Age,City"));
        assert!(result.contains("Alice,30,New York"));
        assert!(result.contains("Bob,25,London"));
    }

    #[test]
    fn test_generate_csv_with_commas_in_fields() {
        let headers = &["Name", "Location"];
        let rows = vec![vec!["Smith, John".into(), "Austin, TX".into()]];
        let result = generate_csv(headers, &rows).unwrap();
        // Fields containing commas should be quoted
        assert!(result.contains("\"Smith, John\""));
    }

    #[test]
    fn test_generate_csv_empty_rows() {
        let headers = &["A", "B"];
        let rows: Vec<Vec<String>> = vec![];
        let result = generate_csv(headers, &rows).unwrap();
        assert!(result.contains("A,B"));
        // Only the header line plus trailing newline
        assert_eq!(result.lines().count(), 1);
    }

    #[test]
    fn test_generate_tsv_basic() {
        let headers = &["X", "Y"];
        let rows = vec![vec!["1".into(), "2".into()]];
        let result = generate_tsv(headers, &rows).unwrap();
        assert!(result.contains("X\tY"));
        assert!(result.contains("1\t2"));
    }

    #[test]
    fn test_parse_csv_roundtrip() {
        let headers = &["Name", "Score"];
        let rows = vec![
            vec!["Alice".into(), "95".into()],
            vec!["Bob".into(), "87".into()],
        ];
        let csv_text = generate_csv(headers, &rows).unwrap();
        let (parsed_headers, parsed_rows) = parse_csv(&csv_text).unwrap();

        assert_eq!(parsed_headers, vec!["Name", "Score"]);
        assert_eq!(parsed_rows.len(), 2);
        assert_eq!(parsed_rows[0], vec!["Alice", "95"]);
        assert_eq!(parsed_rows[1], vec!["Bob", "87"]);
    }

    #[test]
    fn test_parse_csv_with_quotes() {
        let input = "Name,Value\n\"Hello, World\",42\n";
        let (headers, rows) = parse_csv(input).unwrap();
        assert_eq!(headers, vec!["Name", "Value"]);
        assert_eq!(rows[0][0], "Hello, World");
    }
}
