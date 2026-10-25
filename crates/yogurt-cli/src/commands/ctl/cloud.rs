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
            let v: Value = c
                .post("/api/cloud/push", &json!({}))
                .await
                .map_err(with_help)?;
            if json_out {
                println!("{v}");
            } else {
                println!("{}", describe_push(&v));
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
        "15m" => "every 15 minutes",
        "1h" => "every hour",
        "1d" => "every day",
        _ => "off",
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
    println!("auto-push: {}", describe_interval(s("push_interval")));
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
