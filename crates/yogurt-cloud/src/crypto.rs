use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use hkdf::Hkdf;
use p256::ecdh::diffie_hellman;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::pkcs8::EncodePrivateKey;
use p256::{PublicKey, SecretKey};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use crate::{Error, Result};

const KDF_ITER: u32 = 600_000;
#[cfg(test)]
const KDF_ITER_RANGE: std::ops::RangeInclusive<u32> = 100_000..=10_000_000;
#[cfg(test)]
const SALT_LEN: usize = 16;
#[cfg(test)]
const TAG_LEN: usize = 16;
const MIN_PASSPHRASE_CHARS: usize = 12;
const HKDF_INFO: &[u8] = b"yogurt-sync v1";
const POINT_LEN: usize = 65;
const NONCE_LEN: usize = 12;

pub struct KeyPair {
    /// Base64 of the 65-byte uncompressed SEC1 point.
    pub public_key: String,
    /// PKCS#8 DER, the form WebCrypto `importKey("pkcs8")` accepts.
    pub private_pkcs8: Zeroizing<Vec<u8>>,
}

pub fn generate_keypair() -> KeyPair {
    let secret = SecretKey::random(&mut OsRng);
    let public_key = B64.encode(secret.public_key().to_encoded_point(false).as_bytes());
    let private_pkcs8 = secret
        .to_pkcs8_der()
        .expect("P-256 key encodes to PKCS#8")
        .to_bytes();
    KeyPair {
        public_key,
        private_pkcs8,
    }
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    OsRng.fill_bytes(&mut b);
    b
}

fn nfc(passphrase: &str) -> Zeroizing<String> {
    Zeroizing::new(passphrase.nfc().collect())
}

fn passphrase_key(passphrase: &str, salt: &[u8], iter: u32) -> Zeroizing<[u8; 32]> {
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha256>(nfc(passphrase).as_bytes(), salt, iter, &mut *key);
    key
}

/// Returns `(base64(nonce || ciphertext_with_tag), kdf json)`.
/// The passphrase is NFC-normalized, and the base64 public key is the AES-GCM AAD.
pub fn wrap_private_key(
    pkcs8: &[u8],
    public_key_b64: &str,
    passphrase: &str,
) -> Result<(String, serde_json::Value)> {
    if nfc(passphrase).chars().count() < MIN_PASSPHRASE_CHARS {
        return Err(Error::PassphraseTooShort);
    }
    let salt = random::<16>();
    let key = passphrase_key(passphrase, &salt, KDF_ITER);
    let nonce = random::<NONCE_LEN>();
    let ct = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&*key))
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: pkcs8,
                aad: public_key_b64.as_bytes(),
            },
        )
        .map_err(|_| Error::Encrypt)?;
    let wrapped = B64.encode([nonce.as_slice(), &ct].concat());
    let kdf = serde_json::json!({
        "alg": "pbkdf2-sha256",
        "iter": KDF_ITER,
        "salt": B64.encode(salt),
    });
    Ok((wrapped, kdf))
}

pub(crate) fn parse_public_key(b64: &str) -> Result<PublicKey> {
    let raw = B64.decode(b64)?;
    if raw.len() != POINT_LEN {
        return Err(Error::InvalidPublicKey);
    }
    PublicKey::from_sec1_bytes(&raw).map_err(|_| Error::InvalidPublicKey)
}

fn aead_key(shared: &[u8]) -> Zeroizing<[u8; 32]> {
    let mut key = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(None, shared)
        .expand(HKDF_INFO, &mut *key)
        .expect("32 bytes is a valid HKDF-SHA256 length");
    key
}

/// Encrypts `plaintext` to the account public key with a fresh ephemeral key.
///
/// Output is `base64(ephemeral point (65) || nonce (12) || ciphertext_with_tag)`.
/// The ECDH x coordinate feeds HKDF-SHA256 (no salt, info `yogurt-sync v1`) to give the AES-256-GCM key.
/// AAD is `"<meeting id>|<device id>"` for meetings and `"<device id>"` for device names.
pub fn seal(public_key_b64: &str, aad: &str, plaintext: &[u8]) -> Result<String> {
    let account = parse_public_key(public_key_b64)?;
    let eph = SecretKey::random(&mut OsRng);
    let shared = diffie_hellman(eph.to_nonzero_scalar(), account.as_affine());
    let key = aead_key(shared.raw_secret_bytes());
    let nonce = random::<NONCE_LEN>();
    let ct = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&*key))
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| Error::Encrypt)?;
    let point = eph.public_key().to_encoded_point(false);
    Ok(B64.encode([point.as_bytes(), &nonce, &ct].concat()))
}

#[cfg(test)]
pub(crate) fn open(private_pkcs8: &[u8], aad: &str, ct_b64: &str) -> Result<Vec<u8>> {
    use p256::pkcs8::DecodePrivateKey;
    let raw = B64.decode(ct_b64)?;
    if raw.len() < POINT_LEN + NONCE_LEN {
        return Err(Error::MalformedCiphertext);
    }
    let (point, rest) = raw.split_at(POINT_LEN);
    let (nonce, ct) = rest.split_at(NONCE_LEN);
    let eph = PublicKey::from_sec1_bytes(point).map_err(|_| Error::MalformedCiphertext)?;
    let secret = SecretKey::from_pkcs8_der(private_pkcs8).map_err(|_| Error::InvalidPrivateKey)?;
    let shared = diffie_hellman(secret.to_nonzero_scalar(), eph.as_affine());
    let key = aead_key(shared.raw_secret_bytes());
    Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&*key))
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ct,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| Error::Decrypt)
}

#[cfg(test)]
pub(crate) fn unwrap_private_key(
    wrapped: &str,
    kdf: &serde_json::Value,
    public_key_b64: &str,
    passphrase: &str,
) -> Result<Zeroizing<Vec<u8>>> {
    if kdf["alg"] != "pbkdf2-sha256" {
        return Err(Error::UnsupportedKdf);
    }
    let iter = kdf["iter"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| KDF_ITER_RANGE.contains(n))
        .ok_or(Error::UnsupportedKdf)?;
    let salt = B64.decode(kdf["salt"].as_str().ok_or(Error::UnsupportedKdf)?)?;
    if salt.len() != SALT_LEN {
        return Err(Error::UnsupportedKdf);
    }
    let raw = B64.decode(wrapped)?;
    if raw.len() < NONCE_LEN + TAG_LEN + 1 {
        return Err(Error::MalformedCiphertext);
    }
    let (nonce, ct) = raw.split_at(NONCE_LEN);
    let key = passphrase_key(passphrase, &salt, iter);
    Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&*key))
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ct,
                aad: public_key_b64.as_bytes(),
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| Error::Decrypt)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASS: &str = "correct horse battery";

    #[test]
    fn seal_open_round_trip() {
        let kp = generate_keypair();
        let ct = seal(&kp.public_key, "m1|d1", b"hello").unwrap();
        assert_eq!(open(&kp.private_pkcs8, "m1|d1", &ct).unwrap(), b"hello");
    }

    #[test]
    fn wrong_aad_fails() {
        let kp = generate_keypair();
        let ct = seal(&kp.public_key, "m1|d1", b"hello").unwrap();
        assert!(matches!(
            open(&kp.private_pkcs8, "m2|d1", &ct),
            Err(Error::Decrypt)
        ));
    }

    #[test]
    fn wrap_unwrap_round_trip_and_wrong_passphrase() {
        let kp = generate_keypair();
        let (wrapped, kdf) = wrap_private_key(&kp.private_pkcs8, &kp.public_key, PASS).unwrap();
        assert_eq!(kdf["iter"], 600_000);
        let got = unwrap_private_key(&wrapped, &kdf, &kp.public_key, PASS).unwrap();
        assert_eq!(*got, *kp.private_pkcs8);
        assert!(matches!(
            unwrap_private_key(&wrapped, &kdf, &kp.public_key, "wrong passphrase!"),
            Err(Error::Decrypt)
        ));
    }

    #[test]
    fn wrap_is_bound_to_the_public_key() {
        let kp = generate_keypair();
        let (wrapped, kdf) = wrap_private_key(&kp.private_pkcs8, &kp.public_key, PASS).unwrap();
        let other = generate_keypair().public_key;
        assert!(matches!(
            unwrap_private_key(&wrapped, &kdf, &other, PASS),
            Err(Error::Decrypt)
        ));
    }

    #[test]
    fn passphrase_is_nfc_normalized() {
        let kp = generate_keypair();
        let nfd = "cafe\u{301} cre\u{300}me bru\u{302}le\u{301}e";
        let nfc = "caf\u{e9} cr\u{e8}me br\u{fb}l\u{e9}e";
        let (wrapped, kdf) = wrap_private_key(&kp.private_pkcs8, &kp.public_key, nfc).unwrap();
        assert!(unwrap_private_key(&wrapped, &kdf, &kp.public_key, nfd).is_ok());
    }

    #[test]
    fn unwrap_rejects_bad_kdf() {
        let kp = generate_keypair();
        let (wrapped, kdf) = wrap_private_key(&kp.private_pkcs8, &kp.public_key, PASS).unwrap();
        let with = |k: &str, v: serde_json::Value| {
            let mut kdf = kdf.clone();
            kdf[k] = v;
            unwrap_private_key(&wrapped, &kdf, &kp.public_key, PASS)
        };
        for (k, v) in [
            ("alg", serde_json::json!("argon2id")),
            ("iter", serde_json::json!(99_999)),
            ("iter", serde_json::json!(10_000_001)),
            ("iter", serde_json::json!(4_294_967_296u64 + 600_000)),
            ("iter", serde_json::json!(1.5)),
            ("salt", serde_json::json!(B64.encode([0u8; 8]))),
        ] {
            assert!(matches!(with(k, v), Err(Error::UnsupportedKdf)), "{k}");
        }
        let short = B64.encode([0u8; 28]);
        assert!(matches!(
            unwrap_private_key(&short, &kdf, &kp.public_key, PASS),
            Err(Error::MalformedCiphertext)
        ));
    }

    #[test]
    fn short_passphrase_rejected() {
        let kp = generate_keypair();
        for short in ["eleven char", "ééééééééééé"] {
            assert!(matches!(
                wrap_private_key(&kp.private_pkcs8, &kp.public_key, short),
                Err(Error::PassphraseTooShort)
            ));
        }
        assert!(wrap_private_key(&kp.private_pkcs8, &kp.public_key, "éééééééééééé").is_ok());
    }

    #[test]
    fn randomness_differs_per_call() {
        let kp = generate_keypair();
        let a = B64
            .decode(seal(&kp.public_key, "x", b"same").unwrap())
            .unwrap();
        let b = B64
            .decode(seal(&kp.public_key, "x", b"same").unwrap())
            .unwrap();
        assert_ne!(a[..POINT_LEN], b[..POINT_LEN]);
        assert_ne!(
            a[POINT_LEN..POINT_LEN + NONCE_LEN],
            b[POINT_LEN..POINT_LEN + NONCE_LEN]
        );

        let (w1, k1) = wrap_private_key(&kp.private_pkcs8, &kp.public_key, PASS).unwrap();
        let (w2, k2) = wrap_private_key(&kp.private_pkcs8, &kp.public_key, PASS).unwrap();
        assert_ne!(k1["salt"], k2["salt"]);
        assert_ne!(
            B64.decode(w1).unwrap()[..NONCE_LEN],
            B64.decode(w2).unwrap()[..NONCE_LEN]
        );
    }

    #[test]
    fn public_key_is_uncompressed_point() {
        let kp = generate_keypair();
        let raw = B64.decode(&kp.public_key).unwrap();
        assert_eq!((raw.len(), raw[0]), (65, 4));
    }
}
