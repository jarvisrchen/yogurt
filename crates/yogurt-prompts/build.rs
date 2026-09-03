fn main() {
    // Re-run the build if any bundled prompt template changes so that
    // `rust-embed` picks up edits on the next `cargo build`. A directory
    // path makes cargo scan it recursively, which covers `enhance/*.md`.
    println!("cargo:rerun-if-changed=templates");
}
