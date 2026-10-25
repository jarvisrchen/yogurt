use base64::alphabet::URL_SAFE;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use base64::Engine;
use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::crypto::parse_public_key;
use crate::{Error, Result};

const PREFIX: &str = "yogurt-cloud:";

/// Encodes without padding and decodes with or without it.
const B64URL: GeneralPurpose = GeneralPurpose::new(
    &URL_SAFE,
    GeneralPurposeConfig::new()
        .with_encode_padding(false)
        .with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkCode {
    pub url: String,
    pub publishable_key: String,
    pub public_key: String,
}

impl LinkCode {
    fn payload(&self) -> String {
        B64URL.encode(serde_json::to_vec(self).expect("LinkCode serializes"))
    }

    pub fn encode(&self) -> String {
        format!("{PREFIX}{}", self.payload())
    }

    /// The viewer URL is `<viewer>#` followed by this.
    pub fn fragment(&self) -> String {
        format!("c={}", self.payload())
    }

    /// Tolerates whitespace anywhere (hard-wrapped pastes), any prefix case, and padded base64url.
    pub fn decode(s: &str) -> Result<Self> {
        let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        let payload = s
            .get(..PREFIX.len())
            .filter(|p| p.eq_ignore_ascii_case(PREFIX))
            .map(|_| &s[PREFIX.len()..])
            .ok_or(Error::LinkCode("missing yogurt-cloud: prefix"))?;
        let mut code: Self = serde_json::from_slice(&B64URL.decode(payload)?)?;
        code.url = normalize_origin(&code.url)?;
        if code.publishable_key.is_empty() {
            return Err(Error::LinkCode("empty publishable key"));
        }
        parse_public_key(&code.public_key)?;
        Ok(code)
    }
}

fn normalize_origin(raw: &str) -> Result<String> {
    let url = Url::parse(raw).map_err(|_| Error::LinkCode("url does not parse"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::LinkCode("url has credentials"));
    }
    if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
        return Err(Error::LinkCode("url must be an origin"));
    }
    let loopback = match url.host() {
        Some(Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        None => return Err(Error::LinkCode("url has no host")),
    };
    match url.scheme() {
        "https" => {}
        "http" if loopback => {}
        "http" => return Err(Error::LinkCode("http is only allowed for localhost")),
        _ => return Err(Error::LinkCode("url is not http(s)")),
    }
    Ok(url.origin().ascii_serialization())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate_keypair;

    fn sample(url: &str) -> LinkCode {
        LinkCode {
            url: url.into(),
            publishable_key: "sb_publishable_abc".into(),
            public_key: generate_keypair().public_key,
        }
    }

    fn encode_raw(v: &serde_json::Value) -> String {
        format!("{PREFIX}{}", B64URL.encode(v.to_string()))
    }

    #[test]
    fn round_trips() {
        for url in [
            "https://abc.supabase.co",
            "http://127.0.0.1:54321",
            "http://[::1]:54321",
            "http://localhost:54321",
        ] {
            let c = sample(url);
            assert_eq!(LinkCode::decode(&c.encode()).unwrap(), c);
            assert_eq!(c.encode(), format!("{PREFIX}{}", &c.fragment()[2..]));
        }
    }

    #[test]
    fn normalizes_the_origin() {
        let c = sample("https://ABC.supabase.co/");
        assert_eq!(
            LinkCode::decode(&c.encode()).unwrap().url,
            "https://abc.supabase.co"
        );
    }

    #[test]
    fn tolerates_paste_noise() {
        let c = sample("https://abc.supabase.co");
        let wrapped: String = c
            .encode()
            .as_bytes()
            .chunks(20)
            .map(|p| format!("{} \n", std::str::from_utf8(p).unwrap()))
            .collect();
        assert_eq!(LinkCode::decode(&format!("  {wrapped}\t")).unwrap(), c);
        let upper = c.encode().replacen("yogurt-cloud:", "Yogurt-Cloud:", 1);
        assert_eq!(LinkCode::decode(&upper).unwrap(), c);
        let payload = c.encode().split_off(PREFIX.len());
        let padded = format!(
            "{PREFIX}{payload}{}",
            "=".repeat((4 - payload.len() % 4) % 4)
        );
        assert_eq!(LinkCode::decode(&padded).unwrap(), c);
    }

    #[test]
    fn field_names() {
        let c = sample("https://a.supabase.co");
        let json = B64URL.decode(&c.fragment()[2..]).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
        let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["public_key", "publishable_key", "url"]);
    }

    #[test]
    fn rejects_garbage() {
        let good = sample("https://a.supabase.co");
        let json = |url: &str, key: &str, pk: &str| {
            encode_raw(&serde_json::json!({"url": url, "publishable_key": key, "public_key": pk}))
        };
        let ok = &good.public_key;
        let cases = [
            good.encode().replacen("yogurt-cloud:", "other:", 1),
            format!("{PREFIX}!!!not base64!!!"),
            format!("{PREFIX}{}", B64URL.encode("not json")),
            json("https://a.supabase.co", "k", "AAAA"),
            json("ftp://a.supabase.co", "k", ok),
            json("http://example.com", "k", ok),
            json("https://a.supabase.co", "", ok),
            json("https://", "k", ok),
            json("http://localhost:54321@evil.com", "k", ok),
            json("http://127.0.0.1:x@10.0.0.5", "k", ok),
            json("https://user@a.supabase.co", "k", ok),
            json("https://a.supabase.co/rest/v1", "k", ok),
            json("https://a.supabase.co/?x=1", "k", ok),
            json("https://a.supabase.co/#frag", "k", ok),
            String::new(),
        ];
        for c in cases {
            assert!(LinkCode::decode(&c).is_err(), "accepted: {c}");
        }
    }
}
