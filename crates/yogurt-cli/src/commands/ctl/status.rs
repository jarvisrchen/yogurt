use serde::Deserialize;
use serde_json::json;

use super::client::{self, CtlError, Instance, StatusTarget};

#[derive(Debug, Deserialize)]
struct ActiveMeeting {
    id: String,
    title: String,
}

#[derive(Debug, Deserialize)]
struct SettingsView {
    general: yogurt_db::settings::General,
    providers: Vec<ProviderRow>,
}

#[derive(Debug, Deserialize)]
struct ProviderRow {
    name: String,
    is_active: bool,
}

/// Asked of each server, because a grant reaches only processes started after it.
#[derive(Debug, Deserialize)]
struct Grants {
    screen_recording: String,
    microphone: String,
    owner: Option<String>,
}

#[derive(Default)]
struct Details {
    active_meeting: Option<ActiveMeeting>,
    detected_meeting: Option<yogurt_audio::detect::DetectedMeeting>,
    stt: Option<String>,
    provider: Option<String>,
}

pub async fn run(port_flag: Option<u16>, json_out: bool) -> Result<(), CtlError> {
    let http = reqwest::Client::new();
    let target = client::discover_for_status(&http, port_flag).await;

    let instances: Vec<Instance> = match target {
        StatusTarget::Explicit(port, Some(inst)) => {
            let _ = port;
            vec![inst]
        }
        StatusTarget::Explicit(port, None) => return Err(CtlError::NoServer(Some(port))),
        StatusTarget::Scanned(found) if found.is_empty() => return Err(CtlError::NoServer(None)),
        StatusTarget::Scanned(found) => found,
    };

    if instances.len() > 1 {
        let mut rows = Vec::new();
        for inst in &instances {
            rows.push((inst, fetch_grants(inst.port).await));
        }
        if json_out {
            let instances: Vec<_> = rows
                .iter()
                .map(|(inst, g)| {
                    let mut v = json!(inst);
                    v["screen_recording"] = json!(g.as_ref().map(|g| &g.screen_recording));
                    v["microphone"] = json!(g.as_ref().map(|g| &g.microphone));
                    v["permission_owner"] = json!(g.as_ref().and_then(|g| g.owner.as_ref()));
                    v
                })
                .collect();
            println!("{}", json!({ "instances": instances }));
        } else {
            println!("instances:");
            for (inst, g) in &rows {
                let (screen, mic, _) = grant_text(g.as_ref());
                println!(
                    "  127.0.0.1:{} (yogurt {}, {}) screen recording: {screen}, microphone: {mic}",
                    inst.port, inst.version, inst.mode
                );
            }
            println!("help: pass --port <port> to target one instance");
        }
        return Ok(());
    }

    let inst = &instances[0];
    let details = fetch_details(inst.port).await;
    let grants = fetch_grants(inst.port).await;

    if json_out {
        println!(
            "{}",
            json!({
                "instances": [inst],
                "active_meeting": details.active_meeting.as_ref().map(|m| json!({"id": m.id, "title": m.title})),
                "detected_meeting": details.detected_meeting,
                "stt": details.stt,
                "provider": details.provider,
                "screen_recording": grants.as_ref().map(|g| &g.screen_recording),
                "microphone": grants.as_ref().map(|g| &g.microphone),
                "permission_owner": grants.as_ref().and_then(|g| g.owner.as_ref()),
            })
        );
    } else {
        println!(
            "instance: 127.0.0.1:{} (yogurt {}, {})",
            inst.port, inst.version, inst.mode
        );
        match &details.active_meeting {
            Some(m) => println!("active meeting: {} ({})", m.title, m.id),
            None => println!("active meeting: none"),
        }
        match &details.detected_meeting {
            Some(m) => println!("detected meeting: {} ({})", m.app, m.title),
            None => println!("detected meeting: none"),
        }
        println!("stt: {}", details.stt.as_deref().unwrap_or("unknown"));
        println!(
            "provider: {}",
            details.provider.as_deref().unwrap_or("none configured")
        );
        let (screen, mic, owner) = grant_text(grants.as_ref());
        println!("screen recording: {screen}");
        println!("microphone: {mic}");
        println!("permission owner: {owner}");
    }
    Ok(())
}

async fn fetch_details(port: u16) -> Details {
    let mut d = Details::default();
    let Ok(c) = client::Client::at_port(port) else {
        return d;
    };
    if let Ok(am) = c.get::<Option<ActiveMeeting>>("/api/meetings/active").await {
        d.active_meeting = am;
    }
    if let Ok(dm) = c
        .get::<Option<yogurt_audio::detect::DetectedMeeting>>("/api/meetings/detected")
        .await
    {
        d.detected_meeting = dm;
    }
    if let Ok(sv) = c.get::<SettingsView>("/api/settings").await {
        d.stt = Some(format!(
            "{} \u{b7} {}",
            sv.general.stt_provider, sv.general.stt_model
        ));
        d.provider = sv
            .providers
            .into_iter()
            .find(|p| p.is_active)
            .map(|p| p.name);
    }
    d
}

async fn fetch_grants(port: u16) -> Option<Grants> {
    client::Client::at_port(port)
        .ok()?
        .get("/api/audio/permission")
        .await
        .ok()
}

fn grant_text(g: Option<&Grants>) -> (String, String, String) {
    let words = |s: &str| s.replace('_', " ");
    match g {
        Some(g) => (
            words(&g.screen_recording),
            words(&g.microphone),
            g.owner.clone().unwrap_or_else(|| "unknown".into()),
        ),
        None => ("unknown".into(), "unknown".into(), "unknown".into()),
    }
}
