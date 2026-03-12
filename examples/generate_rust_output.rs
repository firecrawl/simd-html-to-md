// Generates Rust converter output for every testdata HTML file.
// Build with: cargo +nightly build --release --example generate_rust_output
// Run from repo root.

use simd_html_to_md::html_to_md;
use std::fs;
use std::path::Path;

fn main() {
    let testdata = Path::new("testdata");
    visit_dir(testdata);
}

fn visit_dir(dir: &Path) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("read_dir {}: {}", dir.display(), e);
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_dir() {
            visit_dir(&path);
        } else if path.extension().map_or(false, |e| e == "html") {
            process_file(&path);
        }
    }
}

fn process_file(path: &Path) {
    let html = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read {}: {}", path.display(), e);
            return;
        }
    };

    let md = html_to_md(&html);

    let out_path = if path.file_name().map_or(false, |n| n == "input.html") {
        path.parent().unwrap().join("rust-output.md")
    } else {
        path.with_extension("rust-output.md")
    };

    if let Err(e) = fs::write(&out_path, &md) {
        eprintln!("write {}: {}", out_path.display(), e);
    } else {
        println!("OK  {}", out_path.display());
    }
}
