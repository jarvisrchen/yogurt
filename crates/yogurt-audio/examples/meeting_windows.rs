//! Dump the on-screen window list with the MTG-11 meeting-detection
//! verdict for each, then the window `detect_meeting` would report.
//!
//! The title patterns in `detect.rs` are vendor UI strings and will drift.
//! When detection misses a real call — or fires on an idle app — run this
//! *during* that call and read the actual title off the offending row:
//!
//! ```text
//! cargo run -p yogurt-audio --example meeting_windows
//! ```
#[cfg(target_os = "macos")]
fn main() {
    use screencapturekit::shareable_content::SCShareableContent;
    use yogurt_audio::detect::{detect_meeting, match_window};

    let content = SCShareableContent::get().expect("SCShareableContent::get");
    for w in content.windows() {
        if !w.is_on_screen() || w.window_layer() != 0 {
            continue;
        }
        let (Some(title), Some(app)) = (w.title(), w.owning_application()) else {
            continue;
        };
        let bundle = app.bundle_identifier();
        let verdict = match_window(&bundle, &title).unwrap_or("-");
        println!("{verdict:<16} {bundle:<40} {title}");
    }
    println!("\ndetect_meeting() -> {:?}", detect_meeting());
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("macOS only");
}
