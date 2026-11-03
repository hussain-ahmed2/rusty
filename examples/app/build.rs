//! Tracks file-based routes so Cargo re-expands `#[rusty::app]` after changes.

use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=rusty.toml");
    visit(Path::new("src"));
}

fn visit(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit(&path);
        }
    }
}
