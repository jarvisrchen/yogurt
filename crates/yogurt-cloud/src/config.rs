//! The Cloud address a user types, and the `config.json` the webapp serves at that address.

use crate::rest::describe;
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// Both values are public by design, so the file needs no secret to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudConfig {
    pub supabase_url: String,
    pub publishable_key: String,
}

#[derive(Deserialize)]
struct V1 {
    supabase_url: String,
    publishable_key: String,
}

/// https, or plain http on a loopback host, which the tests use.
fn scheme_ok(url: &reqwest::Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
        _ => false,
    }
}

/// Turns what the user typed into an origin such as `https://richard-yogurt.vercel.app`. A bare host gets `https`.
pub fn normalize_address(input: &str) -> Result<String> {
    let bad = |m: &str| Error::BadAddress(m.to_string());
    let t = input.trim();
    if t.is_empty() {
        return Err(bad("Enter the Cloud address."));
    }
    let with_scheme = if t.contains("://") {
        t.to_string()
    } else {
        format!("https://{t}")
    };
    let url = reqwest::Url::parse(&with_scheme).map_err(|_| bad("That is not a valid address."))?;
    if url.host_str().is_none() {
        return Err(bad("That is not a valid address."));
    }
    if !scheme_ok(&url) {
        return Err(bad("The Cloud address must use https."));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(bad("Leave the username and password out of the address."));
    }
    Ok(url.origin().ascii_serialization())
}

/// Checks `v` before the shape, so a reshaped future version gets the "newer yogurt" message instead of a parse error.
pub fn parse_config(body: &str) -> Result<CloudConfig> {
    let bad = |m: &str| Error::BadConfig(m.to_string());
    let raw: Value = serde_json::from_str(body)
        .map_err(|_| bad("The address did not return a Yogurt Cloud config.json."))?;
    match raw.get("v").and_then(Value::as_u64) {
        Some(1) => {}
        Some(_) => {
            return Err(bad(
                "This Yogurt Cloud needs a newer yogurt. Update yogurt and try again.",
            ))
        }
        None => {
            return Err(bad(
                "The address did not return a Yogurt Cloud config.json.",
            ))
        }
    }
    let v1: V1 = serde_json::from_value(raw)
        .map_err(|_| bad("The config.json is missing supabase_url or publishable_key."))?;
    let url = reqwest::Url::parse(&v1.supabase_url)
        .ok()
        .filter(scheme_ok)
        .ok_or_else(|| bad("The config.json supabase_url must be an https URL."))?;
    let key = v1.publishable_key.trim();
    if key.is_empty() {
        return Err(bad("The config.json has no publishable_key."));
    }
    Ok(CloudConfig {
        // The origin only: a stray path would otherwise be glued in front of /auth/v1 and /rest/v1.
        supabase_url: url.origin().ascii_serialization(),
        publishable_key: key.to_string(),
    })
}

/// `origin` comes from [`normalize_address`].
pub async fn fetch(origin: &str) -> Result<CloudConfig> {
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()
        .expect("static reqwest client config");
    let resp = http
        .get(format!("{origin}/config.json"))
        .send()
        .await
        .map_err(|e| Error::BadConfig(format!("Could not reach {origin}: {}.", describe(e))))?;
    if !resp.status().is_success() {
        return Err(Error::BadConfig(format!(
            "{origin} answered {} for config.json. Check the address.",
            resp.status().as_u16()
        )));
    }
    let body = resp.text().await.map_err(|e| {
        Error::BadConfig(format!(
            "Could not read config.json from {origin}: {}.",
            describe(e)
        ))
    })?;
    parse_config(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_normalizes_and_enforces_https() {
        let ok = |s: &str| normalize_address(s).unwrap();
        assert_eq!(
            ok("richard-yogurt.vercel.app"),
            "https://richard-yogurt.vercel.app"
        );
        assert_eq!(
            ok("  https://a.vercel.app/some/path?x=1 "),
            "https://a.vercel.app"
        );
        assert_eq!(ok("http://127.0.0.1:5555"), "http://127.0.0.1:5555");
        assert_eq!(ok("http://localhost:3000/"), "http://localhost:3000");
        for bad in [
            "",
            "   ",
            "http://a.vercel.app",
            "ftp://a.vercel.app",
            "https://user:pw@a.vercel.app",
            "https://",
            "not a host",
        ] {
            assert!(
                matches!(normalize_address(bad), Err(Error::BadAddress(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn config_is_validated_and_reduced() {
        let cfg = parse_config(
            r#"{"v":1,"supabase_url":"https://x.supabase.co/rest/v1/","publishable_key":" sb_publishable_abc\n"}"#,
        )
        .unwrap();
        assert_eq!(cfg.supabase_url, "https://x.supabase.co");
        assert_eq!(cfg.publishable_key, "sb_publishable_abc");
        assert!(parse_config(
            r#"{"v":1,"supabase_url":"http://127.0.0.1:56421","publishable_key":"k"}"#
        )
        .is_ok());
        for bad in [
            "",
            "<html>",
            r#"{"supabase_url":"https://x.supabase.co","publishable_key":"k"}"#,
            r#"{"v":1,"supabase_url":"http://x.supabase.co","publishable_key":"k"}"#,
            r#"{"v":1,"supabase_url":"nope","publishable_key":"k"}"#,
            r#"{"v":1,"supabase_url":"https://x.supabase.co","publishable_key":" "}"#,
            r#"{"v":1,"supabase_url":"https://x.supabase.co"}"#,
        ] {
            assert!(
                matches!(parse_config(bad), Err(Error::BadConfig(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_reshaped_newer_version_asks_for_an_update() {
        let Err(Error::BadConfig(m)) = parse_config(r#"{"v":2,"projects":[]}"#) else {
            panic!("expected BadConfig");
        };
        assert!(m.contains("newer yogurt"), "{m}");
    }
}
