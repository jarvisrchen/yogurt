//! Needs a Supabase stack with `schema.sql` applied and `owner.sql` run for `OWNER_EMAIL`. `just test-cloud` does both and sets the three env vars.

use rand::RngCore;
use serde_json::{json, Value};
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

    let library = p
        .rpc_all(&s, "yogurt_library", &json!({ "token": token }), "")
        .await
        .unwrap();
    assert_eq!(library.len(), 1001);
    assert_eq!(library[1000]["meeting_id"], "m1000");
    assert_eq!(library[1000]["body"]["n"], 1000);
    assert_eq!(library[1000]["push_id"], push1);

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
    let args = |at: i64| json!({ "token": token, "at_push": at });
    let after = p
        .rpc_all(
            &s,
            "yogurt_library",
            &json!({ "token": token }),
            "select=meeting_id",
        )
        .await
        .unwrap();
    assert_eq!(after.len(), 1000);
    let as_of_first = p
        .rpc_all(&s, "yogurt_library", &args(push1), "select=meeting_id")
        .await
        .unwrap();
    assert_eq!(as_of_first.len(), 1001);

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
