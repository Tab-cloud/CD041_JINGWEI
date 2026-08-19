//! Checked-in binding generator entrypoint.

use std::path::PathBuf;

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("bindings/typescript"), PathBuf::from);
    if let Err(error) = jingwei_jdp::generate_bindings(&output) {
        eprintln!("codegen failed: {error}");
        std::process::exit(1);
    }
}
