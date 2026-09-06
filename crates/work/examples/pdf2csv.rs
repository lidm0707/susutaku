//! Batch pdf → csv: reads `input/pdf/*.pdf`, writes `output/*.csv`, password from `PASSPDF`.
//!
//! Run: cargo run -p work --example pdf2csv

use std::path::Path;

use work::applications::pdf2csv;

const PDF_DIR: &str = "input/pdf";
const OUT_DIR: &str = "output";

fn main() -> Result<(), String> {
    let count = pdf2csv::convert_dir(Path::new(PDF_DIR), Path::new(OUT_DIR))?;
    println!("converted lines: {count}");
    Ok(())
}
