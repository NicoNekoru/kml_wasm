//! Compile a .kml file to HTML (body snippet). Used to generate showcase-snippet.html and for local testing.
//!
//! Usage:
//!   cargo run --bin compile_kml -- path/to/file.kml
//!   cargo run --bin compile_kml -- < path/to/file.kml

use std::io::Read;

fn offset_to_line_col_and_line(source: &str, offset: usize) -> (usize, usize, String) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (byte_idx, ch) in source.char_indices() {
        if byte_idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    let line_str = source.lines().nth(line.saturating_sub(1)).unwrap_or("").to_string();
    (line, col, line_str)
}

fn main() {
    let source = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("Failed to read {}: {}", path, e);
            std::process::exit(1);
        }),
        None => {
            let mut s = String::new();
            if std::io::stdin().read_to_string(&mut s).is_err() {
                eprintln!("Failed to read stdin");
                std::process::exit(1);
            }
            s
        }
    };
    match kml_wasm::compile_inner(&source) {
        Ok(html) => print!("{}", html),
        Err(e) => {
            let (line, col, line_src) = offset_to_line_col_and_line(&source, e.offset);
            eprintln!(
                "Compile error: {} (line {}, column {}, byte offset {})",
                e.message, line, col, e.offset
            );
            if !line_src.is_empty() {
                eprintln!("{}", line_src);
                // caret under the approximate column (use spaces; col is 1-based)
                if col > 0 {
                    let caret_col = col.saturating_sub(1);
                    eprintln!("{:width$}^", "", width = caret_col);
                }
            }
            std::process::exit(1);
        }
    }
}
