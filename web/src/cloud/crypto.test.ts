// @vitest-environment node
import { describe, expect, it } from "vitest";
import fixture from "./__fixtures__/v1.json";
import { openBlob, unwrapPrivateKey } from "./crypto";

const dec = new TextDecoder();
const unwrap = (passphrase = fixture.passphrase, publicKey = fixture.public_key) =>
  unwrapPrivateKey(fixture.wrapped_private_key, fixture.kdf, publicKey, passphrase);

describe("cloud crypto against the Rust fixture", () => {
  it("decrypts the meeting and the device name", async () => {
    const key = await unwrap();
    expect(key.extractable).toBe(false);
    const meeting = await openBlob(
      key,
      `${fixture.meeting_id}|${fixture.device_id}`,
      fixture.meeting_ct,
    );
    expect(dec.decode(meeting)).toBe(fixture.meeting_plaintext);
    const name = await openBlob(key, fixture.device_id, fixture.device_name_ct);
    expect(dec.decode(name)).toBe(fixture.device_name);
  });

  it("rejects the wrong AAD", async () => {
    const key = await unwrap();
    await expect(
      openBlob(key, `other|${fixture.device_id}`, fixture.meeting_ct),
    ).rejects.toThrow();
  });

  it("rejects the wrong passphrase", async () => {
    await expect(unwrap("not the passphrase")).rejects.toThrow();
  });

  it("accepts the NFD form of the passphrase", async () => {
    expect(fixture.passphrase).toBe(fixture.passphrase.normalize("NFC"));
    const nfd = fixture.passphrase.normalize("NFD");
    expect(nfd).not.toBe(fixture.passphrase);
    await expect(unwrap(nfd)).resolves.toBeDefined();
  });

  it("rejects a different public key", async () => {
    const other = fixture.public_key.replace(/.$/, (c) => (c === "A" ? "B" : "A"));
    await expect(unwrap(fixture.passphrase, other)).rejects.toThrow();
  });

  it("rejects bad kdf parameters and short wrapped keys", async () => {
    const bad = [
      { ...fixture.kdf, alg: "argon2id" },
      { ...fixture.kdf, iter: 99_999 },
      { ...fixture.kdf, iter: 10_000_001 },
      { ...fixture.kdf, iter: 600_000.5 },
      { ...fixture.kdf, salt: btoa("short") },
    ];
    for (const kdf of bad) {
      await expect(
        unwrapPrivateKey(fixture.wrapped_private_key, kdf, fixture.public_key, fixture.passphrase),
      ).rejects.toThrow("unsupported kdf");
    }
    await expect(
      unwrapPrivateKey(btoa("x".repeat(28)), fixture.kdf, fixture.public_key, fixture.passphrase),
    ).rejects.toThrow("malformed");
  });
});
