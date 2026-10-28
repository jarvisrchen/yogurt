//! Yogurt Cloud: status and push. Connecting takes a password, so it stays in the UI: no `ctl` command sets, reveals, or accepts a Cloud secret.

use clap::Subcommand;
use serde_json::{json, Value};

use super::client::{Client, CtlError};

#[derive(Subcommand, Debug)]
pub enum CloudCmd {
    /// Whether this instance is connected, how many meetings are not pushed, and the last push result.
    Status,
    /// Push changed meetings to the connected Yogurt Cloud now.
    Push,
}

pub async fn run(cmd: CloudCmd, port: Option<u16>, json_out: bool) -> Result<(), CtlError> {
    let c = Client::discover(port).await?;
    match cmd {
        CloudCmd::Status => {
            let v: Value = c.get("/api/cloud").await.map_err(with_help)?;
            if json_out {
                println!("{v}");
            } else {
                print_status(&v);
            }
        }
        CloudCmd::Push => {
            let _: Value = c
                .post("/api/cloud/push", &json!({}))
                .await
                .map_err(with_help)?;
            let v = loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let v: Value = c.get("/api/cloud").await.map_err(with_help)?;
                if v["push"].is_null() {
                    break v;
                }
            };
            if json_out {
                println!(
                    "{}",
                    json!({ "last_push": v["last_push"], "last_error": v["last_error"] })
                );
            } else if let Some(err) = v["last_error"]["error"]
                .as_str()
                .filter(|_| v["last_error"]["at"].as_i64() > v["last_push"]["at"].as_i64())
            {
                return Err(CtlError::local(
                    err.to_string(),
                    "check `yogurt ctl cloud status`",
                ));
            } else {
                println!("{}", describe_push(&v["last_push"]));
            }
        }
    }
    Ok(())
}

/// Adds the one place that can fix each failure. The branch is on the server's stable `code`, never on its wording.
fn with_help(e: CtlError) -> CtlError {
    let CtlError::Coded { code, message } = e else {
        return e;
    };
    let help = match code.as_str() {
        "not_connected" => "connect in Settings > Cloud sync, in the browser",
        "session_expired" => "sign in again in Settings > Cloud sync, in the browser",
        "passcode_not_set" => "choose a PIN in the Yogurt Cloud webapp first",
        "device_removed" => "disconnect in Settings > Cloud sync, then connect again",
        _ => "check `yogurt ctl cloud status`",
    };
    CtlError::local(message, help)
}

fn describe_push(v: &Value) -> String {
    let n = |k: &str| v[k].as_i64().unwrap_or(0);
    format!(
        "pushed: {} uploaded, {} deleted",
        n("changed"),
        n("deleted")
    )
}

fn describe_interval(v: &str) -> &'static str {
    match v {
        "meeting" => "after each meeting",
        "15m" => "every 15 minutes",
        "1h" => "every hour",
        "1d" => "every day",
        _ => "off",
    }
}

/// The timer wakes once a minute, so the last minute reads as "about a minute".
fn describe_next(at: Option<i64>, now: i64) -> String {
    let Some(at) = at else {
        return "paused until the last error is fixed".into();
    };
    let min = (at - now + 59_999) / 60_000;
    match (min / 60, min % 60) {
        _ if min <= 1 => "next push in about a minute".into(),
        (0, m) => format!("next push in {m} min"),
        (h, 0) => format!("next push in {h} h"),
        (h, m) => format!("next push in {h} h {m} min"),
    }
}

fn print_status(v: &Value) {
    if v["connected"] != json!(true) {
        println!("cloud: not connected");
        return;
    }
    let s = |k: &str| v[k].as_str().unwrap_or("?");
    println!(
        "cloud: connected as {} to {}",
        s("machine_name"),
        s("address")
    );
    println!("login: {}", s("email"));
    println!(
        "meetings: {} on this Mac, {} not pushed",
        v["meetings"], v["not_pushed"]
    );
    let interval = s("push_interval");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    match interval {
        "off" | "?" => println!("auto-push: off"),
        _ => println!(
            "auto-push: {}, {}",
            describe_interval(interval),
            describe_next(v["next_push"].as_i64(), now)
        ),
    }
    if let Some(sent) = v["push"]["sent"].as_u64() {
        println!("push: running, {sent} of {} sent", v["push"]["total"]);
    }
    let last = &v["last_push"];
    if last.is_null() {
        println!("last push: never");
    } else if let Some(err) = last["error"].as_str() {
        println!(
            "last push: failed ({}): {err}",
            last["code"].as_str().unwrap_or("?")
        );
    } else {
        println!("last push: {}", describe_push(last));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_push_reads_as_a_countdown() {
        const MIN: i64 = 60_000;
        assert_eq!(describe_next(Some(MIN), 0), "next push in about a minute");
        assert_eq!(describe_next(Some(15 * MIN), 0), "next push in 15 min");
        assert_eq!(describe_next(Some(60 * MIN), 0), "next push in 1 h");
        assert_eq!(
            describe_next(Some(23 * 60 * MIN + 30_000), 0),
            "next push in 23 h 1 min"
        );
        assert_eq!(
            describe_next(None, 0),
            "paused until the last error is fixed"
        );
    }

    #[test]
    fn interval_reads_plainly() {
        assert_eq!(describe_interval("1h"), "every hour");
        assert_eq!(describe_interval("off"), "off");
        assert_eq!(describe_interval("?"), "off");
    }

    #[test]
    fn push_summary_reads_counts() {
        assert_eq!(
            describe_push(&json!({"changed": 2, "deleted": 1})),
            "pushed: 2 uploaded, 1 deleted"
        );
    }
}
