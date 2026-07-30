use clap::ValueEnum;
use serde_json::{Map, Value};
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Format {
    Table,
    Tsv,
    Json,
    Jsonl,
}

#[derive(Debug, Default)]
pub struct Table {
    pub headers: Vec<&'static str>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(headers: Vec<&'static str>) -> Self {
        Self {
            headers,
            rows: Vec::new(),
        }
    }
}

pub fn render(
    mut table: Table,
    format: Format,
    full: bool,
    limit: Option<usize>,
) -> io::Result<usize> {
    let total = table.rows.len();
    if let Some(limit) = limit {
        table.rows.truncate(limit);
    }
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    match format {
        Format::Table => render_table(&mut stdout, &table, full)?,
        Format::Tsv => render_tsv(&mut stdout, &table)?,
        Format::Json => {
            let values: Vec<_> = table.rows.iter().map(|row| object(&table, row)).collect();
            serde_json::to_writer_pretty(&mut stdout, &values)?;
            writeln!(stdout)?;
        }
        Format::Jsonl => {
            for row in &table.rows {
                serde_json::to_writer(&mut stdout, &object(&table, row))?;
                writeln!(stdout)?;
            }
        }
    }
    if matches!(format, Format::Table) && table.rows.len() < total {
        writeln!(
            stdout,
            "\nshowing {} of {total} rows (use --limit to change)",
            table.rows.len()
        )?;
    }
    Ok(table.rows.len())
}

fn object(table: &Table, row: &[String]) -> Value {
    let entries = table
        .headers
        .iter()
        .zip(row)
        .map(|(key, value)| ((*key).to_owned(), Value::String(value.clone())));
    Value::Object(entries.collect::<Map<_, _>>())
}

fn clean(value: &str, full: bool) -> String {
    let one_line = value.replace('\t', " ").replace(['\r', '\n'], " ↩ ");
    if full || one_line.chars().count() <= 300 {
        return one_line;
    }
    let mut result: String = one_line.chars().take(299).collect();
    result.push('…');
    result
}

fn render_table(out: &mut impl Write, table: &Table, full: bool) -> io::Result<()> {
    if table.rows.is_empty() {
        return Ok(());
    }
    let rendered: Vec<Vec<String>> = table
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| clean(cell, full)).collect())
        .collect();
    let widths: Vec<usize> = table
        .headers
        .iter()
        .enumerate()
        .map(|(column, header)| {
            rendered
                .iter()
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
                .max(header.len())
        })
        .collect();
    write_row(out, &table.headers, &widths)?;
    let separator: Vec<String> = widths.iter().map(|width| "-".repeat(*width)).collect();
    let separator_refs: Vec<&str> = separator.iter().map(String::as_str).collect();
    write_row(out, &separator_refs, &widths)?;
    for row in &rendered {
        let refs: Vec<&str> = row.iter().map(String::as_str).collect();
        write_row(out, &refs, &widths)?;
    }
    Ok(())
}

fn write_row(out: &mut impl Write, row: &[&str], widths: &[usize]) -> io::Result<()> {
    for (index, cell) in row.iter().enumerate() {
        if index > 0 {
            write!(out, "  ")?;
        }
        if index + 1 == row.len() {
            write!(out, "{cell}")?;
        } else {
            write!(out, "{cell:<width$}", width = widths[index])?;
        }
    }
    writeln!(out)
}

fn render_tsv(out: &mut impl Write, table: &Table) -> io::Result<()> {
    writeln!(out, "{}", table.headers.join("\t"))?;
    for row in &table.rows {
        let cells: Vec<_> = row
            .iter()
            .map(|cell| cell.replace(['\t', '\r', '\n'], " "))
            .collect();
        writeln!(out, "{}", cells.join("\t"))?;
    }
    Ok(())
}
