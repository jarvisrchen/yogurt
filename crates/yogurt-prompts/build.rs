fn main() {
    // Re-run the build if either bundled prompt template changes so that
    // `rust-embed` picks up edits on the next `cargo build`.
    println!("cargo:rerun-if-changed=templates/enhance.md");
    println!("cargo:rerun-if-changed=templates/chat-system.md");
}
