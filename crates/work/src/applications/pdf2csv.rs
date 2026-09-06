use std::fs;
use std::io::Write;
use std::path::Path;

use pdf_rs;

pub const CSV_DELIMITER: char = ',';
pub const CSV_QUOTE: char = '"';
pub const CELL_SPLIT: char = '\t';
pub const ENV_PASSWORD: &str = "PASSPDF";
pub const PDF_EXT: &str = "pdf";
pub const CSV_EXT: &str = "csv";

pub fn convert(pdf_path: &Path, csv_path: &Path) -> Result<usize, String> {
    let pw = env_password();
    convert_encrypted(pdf_path, csv_path, pw.as_deref())
}

pub fn convert_dir(pdf_dir: &Path, out_dir: &Path) -> Result<usize, String> {
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let pw = env_password();
    let mut total = 0;
    for pdf in pdf_files(pdf_dir)? {
        let name = pdf
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("unreadable file name: {}", pdf.display()))?;
        let csv = out_dir.join(name).with_extension(CSV_EXT);
        total += convert_encrypted(&pdf, &csv, pw.as_deref())?;
    }
    Ok(total)
}

fn pdf_files(dir: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    Ok(fs::read_dir(dir)
        .map_err(|e| format!("read dir {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().map(|e| e == PDF_EXT).unwrap_or(false))
        .collect())
}

fn env_password() -> Option<String> {
    std::env::var(ENV_PASSWORD).ok().filter(|p| !p.is_empty())
}

pub fn convert_encrypted(pdf_path: &Path, csv_path: &Path, password: Option<&str>) -> Result<usize, String> {
    let lines = pdf_rs::extract_lines_encrypted(pdf_path, password).map_err(|e| e.to_string())?;
    let mut count = 0;
    let mut out = String::new();
    for line in lines {
        let cells: Vec<&str> = line.split(CELL_SPLIT).collect();
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                out.push(CSV_DELIMITER);
            }
            push_escaped(&mut out, cell);
        }
        out.push('\n');
        count += 1;
    }
    let mut f = fs::File::create(csv_path).map_err(|e| e.to_string())?;
    f.write_all(out.as_bytes()).map_err(|e| e.to_string())?;
    Ok(count)
}

fn push_escaped(out: &mut String, cell: &str) {
    let needs_quotes = cell.contains(CSV_DELIMITER)
        || cell.contains(CSV_QUOTE)
        || cell.contains('\n')
        || cell.contains('\r');
    if needs_quotes {
        out.push(CSV_QUOTE);
        out.push_str(&cell.replace(CSV_QUOTE, "\"\""));
        out.push(CSV_QUOTE);
    } else {
        out.push_str(cell);
    }
}
