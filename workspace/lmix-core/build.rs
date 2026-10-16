//! Embeds `titles/` (the built-in templates, the graphics page, the SDK
//! and Kante's tokens and fonts) so lmix-core serves them without files
//! beside the binary.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read titles dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn main() {
    let root = Path::new("titles").canonicalize().expect("titles dir");
    println!("cargo::rerun-if-changed=titles");
    let mut found = Vec::new();
    files(&root, &mut found);
    found.sort();

    let mut code = String::from("pub static FILES: &[(&str, &[u8])] = &[\n");
    for path in &found {
        println!("cargo::rerun-if-changed={}", path.display());
        let name = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        writeln!(
            code,
            "    ({name:?}, include_bytes!({:?})),",
            path.display().to_string()
        )
        .unwrap();
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("titles.rs");
    std::fs::write(out, code).expect("write titles.rs");
}
