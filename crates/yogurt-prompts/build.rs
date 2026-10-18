fn main() {
    // A directory path makes cargo scan it recursively.
    println!("cargo:rerun-if-changed=templates");
}
