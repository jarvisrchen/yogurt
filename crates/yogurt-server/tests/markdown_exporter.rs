//! `MarkdownExporter` contract: filename `<YYYY-MM-DD-HHmm>-<slug>-<id6>.md` (id6 is the random tail of the id, so same-minute same-slug meetings stay apart), YAML front-matter plus body, atomic write, in-place overwrite, and an `untitled` fallback for blank titles.

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
    // id6 is the last 6 alnum chars of the meeting id ("2ABCD" plus "J" for "01J2ABCD").
    assert!(
        fname.contains("-q3-pricing-sync-") && fname.ends_with("-J2ABCD.md"),
        "filename ends with slug + id6, got {fname}"
    );
    assert!(
        fname.len() > "-q3-pricing-sync-J2ABCD.md".len(),
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
    // An id shorter than 6 chars is used as-is, so the filename ends with -untitled-X.md.
    assert!(
        path.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("-untitled-X.md"),
        "blank title + short id falls back to -untitled-X.md, got {path:?}"
    );
}

const V7_A: &str = "01937a3e-5c1b-7000-8000-aaaaaaaaaaaa";
const V7_B: &str = "01937a3e-5c1b-7000-8000-bbbbbbbbbbbb";

fn meeting<'a>(id: &'a str, body: &'a str) -> me::Meeting<'a> {
    me::Meeting {
        id,
        title: "Meeting Jun 2",
        started_at_unix_ms: 1_782_655_800_000,
        ended_at_unix_ms: None,
        body_md: body,
    }
}

fn md_count(dir: &std::path::Path) -> usize {
    fs::read_dir(dir).unwrap().count()
}

#[test]
fn it_keeps_same_minute_same_title_v7_meetings_apart() {
    let tmp = tempfile::tempdir().unwrap();
    let exporter = me::MarkdownExporter::new(tmp.path().to_path_buf()).unwrap();
    let a = exporter.write(&meeting(V7_A, "a\n")).unwrap();
    let b = exporter.write(&meeting(V7_B, "b\n")).unwrap();
    assert_ne!(a, b);
    assert!(fs::read_to_string(&a).unwrap().ends_with("a\n"));
    assert!(fs::read_to_string(&b).unwrap().ends_with("b\n"));
}

#[test]
fn it_removes_only_its_own_file() {
    let tmp = tempfile::tempdir().unwrap();
    let exporter = me::MarkdownExporter::new(tmp.path().to_path_buf()).unwrap();
    let a = exporter.write(&meeting(V7_A, "a\n")).unwrap();
    let b = exporter.write(&meeting(V7_B, "b\n")).unwrap();
    exporter.remove(&meeting(V7_A, "a\n")).unwrap();
    assert!(!a.exists());
    assert!(b.exists());
}

fn write_legacy(dir: &std::path::Path, id: &str, name: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    fs::write(&p, format!("---\nid: {id}\ntitle: \"x\"\n---\n\nold\n")).unwrap();
    p
}

const LEGACY_NAME: &str = "2026-06-28-1410-meeting-jun-2-01937a.md";

#[test]
fn it_replaces_a_legacy_named_file_on_rewrite() {
    let tmp = tempfile::tempdir().unwrap();
    let exporter = me::MarkdownExporter::new(tmp.path().to_path_buf()).unwrap();
    let legacy = write_legacy(tmp.path(), V7_A, LEGACY_NAME);
    let new = exporter.write(&meeting(V7_A, "a\n")).unwrap();
    assert!(new.exists());
    assert!(!legacy.exists(), "legacy file replaced");
    assert_eq!(md_count(tmp.path()), 1);
}

#[test]
fn it_leaves_a_twins_legacy_file_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let exporter = me::MarkdownExporter::new(tmp.path().to_path_buf()).unwrap();
    let twin = write_legacy(tmp.path(), V7_B, LEGACY_NAME);
    exporter.write(&meeting(V7_A, "a\n")).unwrap();
    assert!(twin.exists(), "write kept the twin's legacy file");
    exporter.remove(&meeting(V7_A, "a\n")).unwrap();
    assert!(twin.exists(), "remove kept the twin's legacy file");
}
