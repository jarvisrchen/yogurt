use std::path::PathBuf;

use serde_json::{json, Value};

use crate::crypto::{open, unwrap_private_key};
use crate::{generate_keypair, seal, wrap_private_key};

const PASSPHRASE: &str = "caf\u{e9} cr\u{e8}me br\u{fb}l\u{e9}e";
const MEETING_ID: &str = "01JBXQ8Z3K5M7N9P2R4T6V8W0Y";
const DEVICE_ID: &str = "3f2b8c1e-5d4a-4e7b-9c6f-1a2b3c4d5e6f";
const DEVICE_NAME: &str = "Richard's MacBook";
const MEETING_PLAINTEXT: &str = r#"{"v":1,"meeting":{"id":"01JBXQ8Z3K5M7N9P2R4T6V8W0Y","title":"Fixture standup","notes":"Ship the ünicode check"},"chat":[]}"#;

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/src/cloud/__fixtures__/v1.json")
}

#[test]
#[ignore = "regenerates web/src/cloud/__fixtures__/v1.json"]
fn write_fixtures() {
    let kp = generate_keypair();
    let (wrapped, kdf) = wrap_private_key(&kp.private_pkcs8, &kp.public_key, PASSPHRASE).unwrap();
    let fixture = json!({
        "passphrase": PASSPHRASE,
        "public_key": kp.public_key,
        "wrapped_private_key": wrapped,
        "kdf": kdf,
        "meeting_id": MEETING_ID,
        "device_id": DEVICE_ID,
        "meeting_ct": seal(&kp.public_key, &format!("{MEETING_ID}|{DEVICE_ID}"), MEETING_PLAINTEXT.as_bytes()).unwrap(),
        "meeting_plaintext": MEETING_PLAINTEXT,
        "device_name_ct": seal(&kp.public_key, DEVICE_ID, DEVICE_NAME.as_bytes()).unwrap(),
        "device_name": DEVICE_NAME,
    });
    let p = path();
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, serde_json::to_string_pretty(&fixture).unwrap() + "\n").unwrap();
}

#[test]
fn checked_in_fixture_opens() {
    let f: Value = serde_json::from_slice(&std::fs::read(path()).unwrap()).unwrap();
    let s = |k: &str| f[k].as_str().unwrap().to_owned();
    let pkcs8 = unwrap_private_key(
        &s("wrapped_private_key"),
        &f["kdf"],
        &s("public_key"),
        &s("passphrase"),
    )
    .unwrap();
    let aad = format!("{}|{}", s("meeting_id"), s("device_id"));
    assert_eq!(
        open(&pkcs8, &aad, &s("meeting_ct")).unwrap(),
        s("meeting_plaintext").as_bytes()
    );
    assert_eq!(
        open(&pkcs8, &s("device_id"), &s("device_name_ct")).unwrap(),
        s("device_name").as_bytes()
    );
}
