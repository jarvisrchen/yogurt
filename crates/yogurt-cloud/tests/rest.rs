//! Needs a Supabase stack with `schema.sql` applied and `owner.sql` run for `OWNER_EMAIL`. `just test-cloud` does both and sets the three env vars.

use rand::RngCore;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Barrier};
use std::time::Duration;
use tokio::sync::watch;
use yogurt_cloud::push::{
    self, Edit, Hooks, LocalMeeting, Pacing, Pass, PhoneMeeting, Plan, Progress,
};
use yogurt_cloud::{Error, Project, Session};

const PASSWORD: &str = "correct horse battery";
const OWNER_EMAIL: &str = "owner@yogurt-cloud.test";

struct Env {
    url: String,
    publishable: String,
    secret: String,
    project: Project,
}

fn env() -> Option<Env> {
    let var = |k| std::env::var(k).ok().filter(|v| !v.is_empty());
    let (Some(url), Some(publishable), Some(secret)) = (
        var("YOGURT_CLOUD_TEST_URL"),
        var("YOGURT_CLOUD_TEST_PUBLISHABLE_KEY"),
        var("YOGURT_CLOUD_TEST_SECRET_KEY"),
    ) else {
        eprintln!("skipping: YOGURT_CLOUD_TEST_* not set, run `just test-cloud`");
        return None;
    };
    Some(Env {
        project: Project::new(&url, &publishable),
        url,
        publishable,
        secret,
    })
}

fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn uuid() -> String {
    let h = random_hex(16);
    format!(
        "{}-{}-4{}-a{}-{}",
        &h[..8],
        &h[8..12],
        &h[13..16],
        &h[17..20],
        &h[20..32]
    )
}

/// Creates a confirmed user through the Auth admin API, which is what a confirmed sign-up amounts to. An existing user is fine.
async fn confirmed_user(env: &Env, email: &str) {
    let resp = reqwest::Client::new()
        .post(format!("{}/auth/v1/admin/users", env.url))
        .header("apikey", &env.secret)
        .bearer_auth(&env.secret)
        .json(&json!({ "email": email, "password": PASSWORD, "email_confirm": true }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.text().await.unwrap();
    assert!(
        status.is_success() || body.contains("email_exists"),
        "{status} {body}"
    );
}

async fn sign_in_as(env: &Env, email: &str) -> Session {
    confirmed_user(env, email).await;
    env.project.sign_in(email, PASSWORD).await.unwrap()
}

async fn owner(env: &Env) -> Session {
    sign_in_as(env, OWNER_EMAIL).await
}

fn stranger() -> String {
    format!("{}@example.com", random_hex(8))
}

#[test]
fn session_debug_hides_tokens() {
    let s = Session {
        access_token: "access-secret".into(),
        refresh_token: "refresh-secret".into(),
        user_id: "u".into(),
        expires_at: 1,
    };
    let dbg = format!("{s:?}");
    assert!(!dbg.contains("secret"), "{dbg}");
}

#[tokio::test]
#[ignore]
async fn credential_errors() {
    let Some(env) = env() else { return };
    let s = owner(&env).await;
    assert!(!s.access_token.is_empty() && !s.user_id.is_empty());

    let wrong = env.project.sign_in(OWNER_EMAIL, "not the password").await;
    assert!(matches!(wrong, Err(Error::BadCredentials)), "{wrong:?}");
    let unknown = env.project.sign_in(&stranger(), PASSWORD).await;
    assert!(matches!(unknown, Err(Error::BadCredentials)), "{unknown:?}");
}

#[tokio::test]
#[ignore]
async fn refresh_rotates_and_rejects_the_old_token() {
    let Some(env) = env() else { return };
    let first = sign_in_as(&env, &stranger()).await;
    let second = env.project.refresh(&first.refresh_token).await.unwrap();
    assert_ne!(second.refresh_token, first.refresh_token);
    assert_eq!(second.user_id, first.user_id);

    // Supabase tolerates one retry of the previous token until its successor is used, so the old token only dies after a second rotation.
    let third = env.project.refresh(&second.refresh_token).await.unwrap();
    assert_ne!(third.refresh_token, second.refresh_token);
    let reused = env.project.refresh(&first.refresh_token).await;
    assert!(matches!(reused, Err(Error::SessionExpired)), "{reused:?}");
    let garbage = env.project.refresh("not-a-token").await;
    assert!(matches!(garbage, Err(Error::SessionExpired)), "{garbage:?}");
}

#[tokio::test]
#[ignore]
async fn dead_access_token_is_session_expired() {
    let Some(env) = env() else { return };
    let mut s = owner(&env).await;
    s.access_token = "not.a.jwt".into();
    let res = env.project.rpc(&s, "yogurt_status", &json!({})).await;
    assert!(matches!(res, Err(Error::SessionExpired)), "{res:?}");
}

#[tokio::test]
#[ignore]
async fn a_stranger_is_not_the_owner_and_can_do_nothing() {
    let Some(env) = env() else { return };
    let s = sign_in_as(&env, &stranger()).await;
    let status = env
        .project
        .rpc(&s, "yogurt_status", &json!({}))
        .await
        .unwrap();
    assert_eq!(status["is_owner"], false);
    assert_eq!(status["owner_email_set"], true);

    let call = env
        .project
        .rpc(
            &s,
            "yogurt_unlock",
            &json!({ "passcode": "whatever you like" }),
        )
        .await;
    assert!(
        matches!(call, Err(Error::Http { status: 403, ref code, .. }) if code == "42501"),
        "{call:?}"
    );
    let catch_up = yogurt_cloud::push::catch_up_search(&env.project, &s).await;
    assert!(
        matches!(catch_up, Err(Error::Http { status: 403, ref code, .. }) if code == "42501"),
        "{catch_up:?}"
    );
    let insert = env
        .project
        .insert(
            &s,
            "yogurt_devices",
            &json!({ "id": uuid(), "name": "evil" }),
        )
        .await;
    assert!(
        matches!(insert, Err(Error::Http { status: 403, .. })),
        "{insert:?}"
    );
}

#[tokio::test]
#[ignore]
async fn owner_pushes_unlocks_and_pages_the_library() {
    let Some(env) = env() else { return };
    let s = owner(&env).await;
    let p = &env.project;

    let status = p.rpc(&s, "yogurt_status", &json!({})).await.unwrap();
    assert_eq!(status["is_owner"], true);
    assert_eq!(status["passcode_set"], false);

    let device = uuid();
    p.insert(
        &s,
        "yogurt_devices",
        &json!({ "id": device, "name": "Test laptop" }),
    )
    .await
    .unwrap();
    let early = p
        .rpc(&s, "yogurt_start_push", &json!({ "device_id": device }))
        .await;
    assert!(
        matches!(early, Err(Error::Http { status: 412, .. })),
        "{early:?}"
    );
    let set = p
        .rpc(
            &s,
            "yogurt_set_passcode",
            &json!({ "new_passcode": "123456" }),
        )
        .await
        .unwrap();
    assert_eq!(set, true);
    let unknown = p
        .rpc(&s, "yogurt_start_push", &json!({ "device_id": uuid() }))
        .await;
    assert!(
        matches!(unknown, Err(Error::Http { status: 404, .. })),
        "{unknown:?}"
    );
    let push1 = p
        .rpc(&s, "yogurt_start_push", &json!({ "device_id": device }))
        .await
        .unwrap()
        .as_i64()
        .unwrap();

    let versions: Vec<Value> = (0..1001)
        .map(|i| json!({ "meeting_id": format!("m{i:04}"), "push_id": push1, "device_id": device, "body": { "n": i } }))
        .collect();
    for batch in versions.chunks(400) {
        p.insert(&s, "yogurt_versions", &Value::Array(batch.to_vec()))
            .await
            .unwrap();
    }

    assert_eq!(yogurt_cloud::push::catch_up_search(p, &s).await.unwrap(), 0);

    let live = p
        .rpc_all(
            &s,
            "yogurt_device_meetings",
            &json!({ "device_id": device }),
            "",
        )
        .await
        .unwrap();
    assert_eq!(live.len(), 1001);

    let wrong = p
        .rpc(
            &s,
            "yogurt_unlock",
            &json!({ "passcode": "wrong passcode" }),
        )
        .await
        .unwrap();
    assert_eq!(wrong, Value::Null);
    let token = p
        .rpc(
            &s,
            "yogurt_unlock",
            &json!({ "passcode": "123456", "remember": false, "label": "cargo test" }),
        )
        .await
        .unwrap();
    let token = token.as_str().unwrap();

    let page = |at: Option<i64>, offset: i64| json!({ "token": token, "at_push": at, "page_limit": 100, "page_offset": offset });
    let first = p
        .rpc(&s, "yogurt_library", &page(None, 1000))
        .await
        .unwrap();
    let first = first.as_array().unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0]["meeting_id"], "m1000");
    assert_eq!(first[0]["total"], 1001);
    assert_eq!(first[0]["push_id"], push1);
    assert!(first[0].get("body").is_none());
    let body = p
        .rpc(
            &s,
            "yogurt_meeting",
            &json!({ "token": token, "device_id": device, "meeting_id": "m1000" }),
        )
        .await
        .unwrap();
    assert_eq!(body[0]["body"]["n"], 1000);
    assert_eq!(body[0]["push_id"], push1);

    let push2 = p
        .rpc(&s, "yogurt_start_push", &json!({ "device_id": device }))
        .await
        .unwrap()
        .as_i64()
        .unwrap();
    p.insert(
        &s,
        "yogurt_versions",
        &json!({ "meeting_id": "m0000", "push_id": push2, "device_id": device, "body": null }),
    )
    .await
    .unwrap();
    let after = p.rpc(&s, "yogurt_library", &page(None, 0)).await.unwrap();
    assert_eq!(after[0]["total"], 1000);
    assert_eq!(after[0]["meeting_id"], "m0001");
    let as_of_first = p
        .rpc(&s, "yogurt_library", &page(Some(push1), 0))
        .await
        .unwrap();
    assert_eq!(as_of_first[0]["total"], 1001);
    assert_eq!(as_of_first[0]["meeting_id"], "m0000");
    let gone = p
        .rpc(
            &s,
            "yogurt_meeting",
            &json!({ "token": token, "device_id": device, "meeting_id": "m0000" }),
        )
        .await
        .unwrap();
    assert!(gone.as_array().unwrap().is_empty());
    let then = p
        .rpc(
            &s,
            "yogurt_meeting",
            &json!({ "token": token, "device_id": device, "meeting_id": "m0000", "at_push": push1 }),
        )
        .await
        .unwrap();
    assert_eq!(then[0]["body"]["n"], 0);

    let history = p
        .rpc_all(&s, "yogurt_history", &json!({ "token": token }), "")
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["push_id"], push2);
    assert_eq!(history[0]["deleted"], 1);
    assert_eq!(history[1]["changed"], 1001);

    let bad = p
        .rpc(&s, "yogurt_library", &json!({ "token": "forged" }))
        .await;
    assert!(
        matches!(bad, Err(Error::Http { status: 403, ref code, .. }) if code == "28000"),
        "{bad:?}"
    );
}

/// The Cloud login alone reads no meeting through a plain PostgREST GET.
#[tokio::test]
#[ignore]
async fn owner_session_reads_no_table() {
    let Some(env) = env() else { return };
    let s = owner(&env).await;
    for table in [
        "yogurt_versions",
        "yogurt_pushes",
        "yogurt_devices",
        "yogurt_owner",
        "yogurt_browsers",
    ] {
        let resp = reqwest::Client::new()
            .get(format!("{}/rest/v1/{table}?select=*", env.url))
            .header("apikey", &env.publishable)
            .bearer_auth(&s.access_token)
            .send()
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap();
        eprintln!("GET {table}: {status} {body}");
        assert_eq!(status, 403, "{table}: {body}");
        assert_eq!(body["code"], "42501", "{table}: {body}");
    }
}

/// A Mac's meetings in memory, standing in for its SQLite.
#[derive(Default)]
struct Mac {
    here: HashMap<String, Value>,
    hashes: HashMap<String, String>,
    unadopted: Vec<String>,
    /// Holds `adopt` for this meeting until the other Mac has listed it too.
    gate: Option<(String, Arc<Barrier>)>,
}

impl Hooks for Mac {
    fn bundle(&mut self, id: &str) -> Result<Option<Value>, String> {
        Ok(self.here.get(id).cloned())
    }
    fn refreshed(&mut self, _: &Session) -> Result<(), String> {
        Ok(())
    }
    fn stored(&mut self, hashes: &[(String, String)]) -> Result<(), String> {
        self.hashes.extend(hashes.iter().cloned());
        Ok(())
    }
    fn progress(&mut self, _: &Progress) {}
    fn adopt(&mut self, phone: &PhoneMeeting) -> Result<bool, String> {
        if let Some((id, gate)) = &self.gate {
            if *id == phone.meeting_id {
                gate.wait();
            }
        }
        if self.here.contains_key(&phone.meeting_id) {
            return Ok(false);
        }
        self.here
            .insert(phone.meeting_id.clone(), phone.body.clone());
        Ok(true)
    }
    fn unadopt(&mut self, id: &str) -> Result<(), String> {
        self.here.remove(id);
        self.unadopted.push(id.into());
        Ok(())
    }
    fn edit(&mut self, _: &Edit) -> Result<bool, String> {
        Ok(true)
    }
}

fn fast() -> Pacing {
    Pacing {
        pause: Duration::from_millis(10),
        retries: vec![Duration::from_millis(100); 2],
        ..Pacing::default()
    }
}

/// The adopt step, then the pass that pushes this Mac's meetings. Returns what each did.
async fn mac_pass(
    project: &Project,
    session: &mut Session,
    device: &str,
    mac: &mut Mac,
) -> yogurt_cloud::Result<(Vec<String>, Plan)> {
    let (_tx, mut rx) = watch::channel(false);
    let pacing = fast();
    let adopted = push::adopt(project, &pacing, session, &mut rx, mac, device).await?;
    let local: Vec<LocalMeeting> = mac
        .here
        .iter()
        .map(|(id, body)| LocalMeeting::new(id.clone(), mac.hashes.get(id).cloned(), body))
        .collect();
    let plan = Pass {
        project,
        device_id: device,
        local: &local,
        skip: None,
        pacing: &pacing,
    }
    .run(session, &mut rx, mac)
    .await?;
    Ok((adopted, plan))
}

fn phone_bundle(id: &str, ended_at: Option<i64>) -> Value {
    json!({ "v": 1, "chat": [], "meeting": {
        "id": id, "title": format!("Phone {id}"), "started_at": 1_000, "ended_at": ended_at,
        "notes_md": "", "enriched_md": null, "transcript_json": "[]", "starred": false,
        "stt_engine": "cloud · nova-3 · phone", "llm_model": null, "template": null, "labels": [],
    }})
}

async fn device(p: &Project, s: &Session, name: &str, kind: &str) -> String {
    let id = uuid();
    p.insert(
        s,
        "yogurt_devices",
        &json!({ "id": id, "name": name, "kind": kind }),
    )
    .await
    .unwrap();
    id
}

/// What the phone does every 30 seconds: one push carrying its meetings whole.
async fn phone_push(p: &Project, s: &Session, phone: &str, meetings: &[(&str, Option<i64>)]) {
    let push = p
        .rpc(s, "yogurt_start_push", &json!({ "device_id": phone }))
        .await
        .unwrap();
    let rows: Vec<Value> = meetings
        .iter()
        .map(|(id, ended)| json!({ "meeting_id": id, "push_id": push, "device_id": phone, "body": phone_bundle(id, *ended) }))
        .collect();
    p.insert(s, "yogurt_versions", &Value::Array(rows))
        .await
        .unwrap();
}

async fn live(p: &Project, s: &Session, device: &str) -> Vec<Value> {
    p.rpc_all(
        s,
        "yogurt_device_meetings",
        &json!({ "device_id": device }),
        "",
    )
    .await
    .unwrap()
}

/// Sorted after `owner_pushes_unlocks_and_pages_the_library`, which expects to set the first passcode.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn phone_meetings_are_adopted_by_one_mac() {
    let Some(env) = env() else { return };
    let mut s = owner(&env).await;
    let p = &env.project;
    let status = p.rpc(&s, "yogurt_status", &json!({})).await.unwrap();
    if status["passcode_set"] != true {
        p.rpc(
            &s,
            "yogurt_set_passcode",
            &json!({ "new_passcode": "123456" }),
        )
        .await
        .unwrap();
    }
    let phone = device(p, &s, "Test phone", "phone").await;
    let mac_a = device(p, &s, "Mac A", "mac").await;
    let ended = format!("ended-{}", random_hex(4));
    let open = format!("open-{}", random_hex(4));
    phone_push(p, &s, &phone, &[(&ended, Some(9_000)), (&open, None)]).await;

    let mut a = Mac::default();
    let (adopted, plan) = mac_pass(p, &mut s, &mac_a, &mut a).await.unwrap();
    assert_eq!(
        adopted,
        std::slice::from_ref(&ended),
        "only the ended meeting"
    );
    assert_eq!(
        plan.changed,
        std::slice::from_ref(&ended),
        "and the pass pushes it"
    );
    assert_eq!(a.here[&ended], phone_bundle(&ended, Some(9_000)));
    assert_eq!(
        live(p, &s, &phone).await,
        [json!(open)],
        "the phone's copy is marked deleted"
    );
    assert_eq!(
        live(p, &s, &mac_a).await,
        [json!(ended)],
        "and the Mac's copy is live"
    );

    let (adopted, plan) = mac_pass(p, &mut s, &mac_a, &mut a).await.unwrap();
    assert!(
        adopted.is_empty() && plan == Plan::default(),
        "a rerun does nothing: {adopted:?} {plan:?}"
    );

    // Two Macs list the same meeting, then both claim it.
    let race = format!("race-{}", random_hex(4));
    phone_push(p, &s, &phone, &[(&race, Some(9_500))]).await;
    let mac_b = device(p, &s, "Mac B", "mac").await;
    let mac_c = device(p, &s, "Mac C", "mac").await;
    let gate = Arc::new(Barrier::new(2));
    let run = |device: String, session: Session| {
        let gate = gate.clone();
        let race = race.clone();
        let project = Project::new(&env.url, &env.publishable);
        tokio::spawn(async move {
            let mut session = session;
            let mut mac = Mac {
                gate: Some((race, gate)),
                ..Mac::default()
            };
            let res = mac_pass(&project, &mut session, &device, &mut mac).await;
            (res.map(|(adopted, _)| adopted).unwrap(), mac)
        })
    };
    let (one, two) = tokio::join!(run(mac_b.clone(), s.clone()), run(mac_c.clone(), s.clone()));
    let ((won_b, mac_one), (won_c, mac_two)) = (one.unwrap(), two.unwrap());
    let (winner, loser) = match (won_b.contains(&race), won_c.contains(&race)) {
        (true, false) => (mac_one, mac_two),
        (false, true) => (mac_two, mac_one),
        other => panic!("exactly one Mac adopts it, got {other:?}"),
    };
    assert!(winner.here.contains_key(&race));
    assert!(
        !loser.here.contains_key(&race),
        "the loser removes its copy"
    );
    assert_eq!(loser.unadopted, std::slice::from_ref(&race));
    let mut copies = 0;
    for d in [&mac_a, &mac_b, &mac_c, &phone] {
        copies += live(p, &s, d).await.contains(&json!(race)) as usize;
    }
    assert_eq!(copies, 1, "the Cloud holds the meeting once");

    let stranger = sign_in_as(&env, &stranger()).await;
    for (f, args) in [
        ("yogurt_phone_meetings", json!({})),
        (
            "yogurt_adopt",
            json!({ "device_id": phone, "meeting_id": open, "push_id": 1 }),
        ),
    ] {
        let res = p.rpc(&stranger, f, &args).await;
        assert!(
            matches!(res, Err(Error::Http { status: 403, ref code, .. }) if code == "42501"),
            "{f}: {res:?}"
        );
    }
}
