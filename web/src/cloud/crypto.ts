// Mirrors crates/yogurt-cloud/src/crypto.rs. WebCrypto only, no library.

export interface Kdf {
  alg: string;
  iter: number;
  salt: string;
}

const POINT_LEN = 65;
const NONCE_LEN = 12;
const enc = new TextEncoder();

function b64decode(s: string): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
}

const MIN_ITER = 100_000;
const MAX_ITER = 10_000_000;
const SALT_LEN = 16;
const TAG_LEN = 16;

export async function unwrapPrivateKey(
  wrapped: string,
  kdf: Kdf,
  publicKey: string,
  passphrase: string,
): Promise<CryptoKey> {
  if (kdf.alg !== "pbkdf2-sha256") throw new Error(`unsupported kdf: ${kdf.alg}`);
  if (!Number.isInteger(kdf.iter) || kdf.iter < MIN_ITER || kdf.iter > MAX_ITER) {
    throw new Error("unsupported kdf: iteration count out of range");
  }
  const salt = b64decode(kdf.salt);
  if (salt.length !== SALT_LEN) throw new Error("unsupported kdf: bad salt");
  const raw = b64decode(wrapped);
  if (raw.length < NONCE_LEN + TAG_LEN + 1) throw new Error("malformed wrapped key");
  const base = await crypto.subtle.importKey(
    "raw",
    enc.encode(passphrase.normalize("NFC")),
    "PBKDF2",
    false,
    ["deriveKey"],
  );
  const aes = await crypto.subtle.deriveKey(
    { name: "PBKDF2", hash: "SHA-256", salt, iterations: kdf.iter },
    base,
    { name: "AES-GCM", length: 256 },
    false,
    ["decrypt"],
  );
  const pkcs8 = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: raw.slice(0, NONCE_LEN), additionalData: enc.encode(publicKey) },
    aes,
    raw.slice(NONCE_LEN),
  );
  return crypto.subtle.importKey("pkcs8", pkcs8, { name: "ECDH", namedCurve: "P-256" }, false, [
    "deriveBits",
  ]);
}

export async function openBlob(
  privateKey: CryptoKey,
  aad: string,
  ctB64: string,
): Promise<Uint8Array> {
  const raw = b64decode(ctB64);
  if (raw.length < POINT_LEN + NONCE_LEN) throw new Error("malformed ciphertext");
  const eph = await crypto.subtle.importKey(
    "raw",
    raw.slice(0, POINT_LEN),
    { name: "ECDH", namedCurve: "P-256" },
    false,
    [],
  );
  const shared = await crypto.subtle.deriveBits({ name: "ECDH", public: eph }, privateKey, 256);
  const hkdf = await crypto.subtle.importKey("raw", shared, "HKDF", false, ["deriveKey"]);
  const aes = await crypto.subtle.deriveKey(
    { name: "HKDF", hash: "SHA-256", salt: new Uint8Array(0), info: enc.encode("yogurt-sync v1") },
    hkdf,
    { name: "AES-GCM", length: 256 },
    false,
    ["decrypt"],
  );
  const pt = await crypto.subtle.decrypt(
    {
      name: "AES-GCM",
      iv: raw.slice(POINT_LEN, POINT_LEN + NONCE_LEN),
      additionalData: enc.encode(aad),
    },
    aes,
    raw.slice(POINT_LEN + NONCE_LEN),
  );
  return new Uint8Array(pt);
}
