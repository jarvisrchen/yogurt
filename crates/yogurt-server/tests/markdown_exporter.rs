//! Integration tests for `MarkdownExporter` — STORE-03 + STORE-04 contract:
//!   1. Filename format `<YYYY-MM-DD-HHmm>-<slug>-<id6>.md`
//!      (HI-6: id6 suffix prevents same-minute same-slug collisions)
//!   2. YAML front-matter (id / title / started_at / ended_at) + body
//!   3. Atomic write (no stray `.tmp` left behind)
//!   4. Repeat writes overwrite in place (same path for same meeting)
//!   5. Empty/blank title falls back to `untitled`

use std::fs;
use yogurt_server::__test_only_markdown_exporter as me;

#[test]
fn it_writes_atomic_yaml_markdown_with_dated_slug_filename() {
    let tmp = tempfile::tempdir().unwrap();
    let exporter = me::MarkdownExporter::new(tmp.path().to_path_buf()).unwrap();

    let m = me::Meeting {
        id: "01J2ABCD",
        title: "Q3 Pricing Sync",
        started_at_unix_ms: 1_782_655_800_000,
        ended_at_unix_ms: Some(1_782_658_800_000),
        body_md: "- pricing\n- timeline\n",
    };
    let path = exporter.write(&m).unwrap();

    assert!(path.exists(), "final file written");
    let fname = path.file_name().unwrap().to_str().unwrap();
    // HI-6: filename now `<stamp>-<slug>-<id6>.md` — id6 is first 6
    // alnum chars of meeting id ("01J2AB" for "01J2ABCD").
    assert!(
        fname.contains("-q3-pricing-sync-") && fname.ends_with("-01J2AB.md"),
        "filename ends with slug + id6, got {fname}"
    );
    // Filename pattern: YYYY-MM-DD-HHmm- prefix (15 chars) before the slug.
    assert!(
        fname.len() > "-q3-pricing-sync-01J2AB.md".len(),
        "filename has dated prefix, got {fname}"
    );

    let body = fs::read_to_string(&path).unwrap();
    assert!(body.starts_with("---\n"), "yaml front-matter present");
    assert!(body.contains("id: 01J2ABCD"));
    assert!(body.contains("title: \"Q3 Pricing Sync\""));
    assert!(body.contains("started_at: 1782655800000"));
    assert!(body.contains("ended_at: 1782658800000"));
    assert!(body.contains("- pricing"));
    assert!(body.contains("- timeline"));

    // No leftover .tmp file (atomicity proof — rename consumed the tmp).
    let entries: Vec<_> = fs::read_dir(tmp.path()).unwrap().collect();
    assert_eq!(entries.len(), 1, "no .tmp left behind, got {entries:?}");
}

#[test]
fn it_overwrites_on_repeat_write_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    let exporter = me::MarkdownExporter::new(tmp.path().to_path_buf()).unwrap();
    let mut m = me::Meeting {
        id: "01J2ABCD",
        title: "Sync",
        started_at_unix_ms: 1_782_655_800_000,
        ended_at_unix_ms: None,
        body_md: "v1\n",
    };
    let p1 = exporter.write(&m).unwrap();
    m.body_md = "v2\n";
    let p2 = exporter.write(&m).unwrap();
    assert_eq!(p1, p2, "same path for same meeting (id+title+ts)");
    let body = fs::read_to_string(&p1).unwrap();
    assert!(
        body.contains("v2") && !body.contains("v1"),
        "second write overwrote, got:\n{body}"
    );
    assert!(body.contains("ended_at: null"), "null ended_at rendered");

    // Still only one file (no .tmp orphan).
    let entries: Vec<_> = fs::read_dir(tmp.path()).unwrap().collect();
    assert_eq!(entries.len(), 1);
}

#[test]
fn it_falls_back_to_untitled_for_empty_title() {
    let tmp = tempfile::tempdir().unwrap();
    let exporter = me::MarkdownExporter::new(tmp.path().to_path_buf()).unwrap();
    let m = me::Meeting {
        id: "X",
        title: "  ",
        started_at_unix_ms: 1_782_655_800_000,
        ended_at_unix_ms: None,
        body_md: "",
    };
    let path = exporter.write(&m).unwrap();
    // HI-6: id "X" is shorter than 6 chars → fallback to the id as-is.
    // Filename ends with -untitled-X.md.
    assert!(
        path.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("-untitled-X.md"),
        "blank title + short id falls back to -untitled-X.md, got {path:?}"
    );
}
