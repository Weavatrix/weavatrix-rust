//! Caller-supplied measurement series.
//!
//! The harness that produced the numbers owns their format, so this reads a
//! tab- or comma-separated table from inside the repository, ignores `#`
//! comment rows, and records every row it could not use instead of silently
//! shortening the series.

use crate::operations::health::paths::read_contained;
use std::path::Path;

/// One usable row: a revision and the number measured at it.
pub(super) struct Measurement {
    pub row: usize,
    pub revision: String,
    pub value: f64,
}

/// A parsed measurement table, including the rows it could not use.
pub(super) struct Series {
    pub columns: Vec<String>,
    pub data_rows: usize,
    pub measurements: Vec<Measurement>,
    pub skipped: Vec<(usize, String)>,
}

pub(super) fn read(
    root: &Path,
    relative: &str,
    revision_column: &str,
    metric: &str,
) -> Result<Series, String> {
    let text = read_contained(root, relative, "measurements_file")?;
    let mut rows = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !is_ignorable(line));
    let (_, header) = rows
        .next()
        .ok_or_else(|| format!("{relative} has no header row"))?;
    let delimiter = delimiter_of(header);
    let columns = cells(header, delimiter);
    let revision_at = column_index(&columns, revision_column)
        .ok_or_else(|| missing_column(relative, revision_column, &columns))?;
    let metric_at =
        column_index(&columns, metric).ok_or_else(|| missing_column(relative, metric, &columns))?;

    let mut series = Series {
        columns,
        data_rows: 0,
        measurements: Vec::new(),
        skipped: Vec::new(),
    };
    for (offset, line) in rows {
        series.data_rows += 1;
        let row = offset + 1;
        let values = cells(line, delimiter);
        let revision = values.get(revision_at).map_or("", String::as_str);
        if revision.is_empty() {
            series
                .skipped
                .push((row, format!("column {revision_column} is empty")));
            continue;
        }
        let Some(value) = values.get(metric_at).and_then(|cell| number(cell)) else {
            series
                .skipped
                .push((row, format!("column {metric} is not a finite number")));
            continue;
        };
        series.measurements.push(Measurement {
            row,
            revision: revision.to_owned(),
            value,
        });
    }
    Ok(series)
}

fn missing_column(relative: &str, name: &str, columns: &[String]) -> String {
    format!(
        "{relative} has no column {name:?}; available columns: {}",
        columns.join(", ")
    )
}

/// Exact header match first, so a table carrying both `time` and `TIME`
/// resolves to the one the caller actually named.
fn column_index(columns: &[String], name: &str) -> Option<usize> {
    columns
        .iter()
        .position(|column| column == name)
        .or_else(|| {
            columns
                .iter()
                .position(|column| column.eq_ignore_ascii_case(name))
        })
}

/// A measured value, or nothing when the harness left the cell blank or
/// marked the run as having produced no number.
fn number(cell: &str) -> Option<f64> {
    let text = cell.trim();
    if text.is_empty() || text == "-" {
        return None;
    }
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn is_ignorable(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// Tabs win ties, so a single-column file stays one column instead of
/// splitting on a comma inside free text.
fn delimiter_of(header: &str) -> char {
    if header.matches('\t').count() >= header.matches(',').count() {
        '\t'
    } else {
        ','
    }
}

/// Splits one row honouring double quotes, so a separator inside a quoted
/// description cannot shift the columns that follow it.
fn cells(line: &str, delimiter: char) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in line.chars() {
        if character == '"' {
            quoted = !quoted;
        } else if character == delimiter && !quoted {
            values.push(current.trim().to_owned());
            current.clear();
        } else {
            current.push(character);
        }
    }
    values.push(current.trim().to_owned());
    values
}
