//! `POST /api/chat` against `MockChunksLlm`: retrieval picks the prompt's meetings and the NDJSON stream carries sources then deltas.

use yogurt_db::{MeetingPatch, NewMeeting};
use yogurt_server::test_support::run_with_mock_llm;

#[tokio::test(flavor = "multi_thread")]
async fn it_answers_from_the_matching_meetings_and_streams_sources_first() {
    let (srv, server_task) = run_with_mock_llm(&["Twenty dollars ", "[1]."])
        .await
        .expect("boot server");
    let repo = &srv.state.meeting_repo;
    let mk = |title: &str, transcript: &str| {
        let m = repo
            .create(NewMeeting {
                title: title.into(),
                ..Default::default()
            })
            .unwrap();
        repo.patch(
            &m.id,
            MeetingPatch {
                transcript_json: Some(serde_json::json!([{ "text": transcript }]).to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        m.id
    };
    let pricing = mk(
        "Pricing sync",
        "Steven settled the pricing at twenty dollars",
    );
    for i in 0..8 {
        mk(&format!("Standup {i}"), "nothing relevant here");
    }

    let resp = reqwest::Client::new()
        .post(format!("http://{}/api/chat?token={}", srv.addr, srv.token))
        .json(&serde_json::json!({
            "messages": [{ "role": "user", "content": "What did Steven decide about pricing?" }]
        }))
        .send()
        .await
        .expect("post");
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers()["content-type"], "application/x-ndjson");
    let text = resp.text().await.unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let sources = lines[0]["sources"].as_array().expect("sources first");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["id"], pricing.as_str());
    let answer: String = lines[1..]
        .iter()
        .map(|l| l["delta"].as_str().unwrap())
        .collect();
    assert_eq!(answer, "Twenty dollars [1].");

    let reqs = srv.llm.requests.lock().unwrap();
    let prompt: String = reqs[0]
        .messages
        .iter()
        .map(|m| m.content.as_str())
        .collect();
    assert!(prompt.contains("[1] Pricing sync"), "{prompt}");
    assert!(prompt.contains("settled the pricing"), "{prompt}");
    assert!(
        !prompt.contains("Standup"),
        "only matching meetings go in the prompt"
    );
    assert_eq!(reqs[0].messages.last().unwrap().role, "user");
    drop(reqs);

    server_task.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_a_conversation_not_ending_in_a_user_question() {
    let (srv, server_task) = run_with_mock_llm(&[]).await.expect("boot server");
    let url = format!("http://{}/api/chat?token={}", srv.addr, srv.token);
    let client = reqwest::Client::new();
    for messages in [
        serde_json::json!([]),
        serde_json::json!([{ "role": "assistant", "content": "hi" }]),
        serde_json::json!([{ "role": "system", "content": "x" }, { "role": "user", "content": "q" }]),
    ] {
        let resp = client
            .post(&url)
            .json(&serde_json::json!({ "messages": messages }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400, "{messages}");
    }
    server_task.abort();
}
