//! `yogurt ctl status` (and bare `yogurt ctl`) -- content-first: what
//! instance(s) are up, what's recording, what detection sees, which
//! provider/engine is active, and whether the OS grants are in place.

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

    let (screen, mic) = grants();

    if instances.len() > 1 {
        if json_out {
            println!(
                "{}",
                json!({
                    "instances": instances,
                    "screen_recording": screen,
                    "microphone": mic,
                })
            );
        } else {
            println!("instances:");
            for i in &instances {
                println!("  127.0.0.1:{} (yogurt {}, {})", i.port, i.version, i.mode);
            }
            println!("screen recording: {screen}");
            println!("microphone: {mic}");
            println!("help: pass --port <port> to target one instance");
        }
        return Ok(());
    }

    let inst = &instances[0];
    let details = fetch_details(inst.port).await;

    if json_out {
        println!(
            "{}",
            json!({
                "instances": [inst],
                "active_meeting": details.active_meeting.as_ref().map(|m| json!({"id": m.id, "title": m.title})),
                "detected_meeting": details.detected_meeting,
                "stt": details.stt,
                "provider": details.provider,
                "screen_recording": screen,
                "microphone": mic,
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
        println!("screen recording: {screen}");
        println!("microphone: {mic}");
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

fn grants() -> (&'static str, &'static str) {
    use yogurt_audio::permission::{has_microphone_permission, has_screen_recording_permission};
    (
        permission_str(has_screen_recording_permission()),
        permission_str(has_microphone_permission()),
    )
}

fn permission_str(p: yogurt_audio::permission::PermissionStatus) -> &'static str {
    use yogurt_audio::permission::PermissionStatus::*;
    match p {
        Granted => "granted",
        Denied => "denied",
        NotDetermined => "not determined",
        NotRequired => "not required",
    }
}
