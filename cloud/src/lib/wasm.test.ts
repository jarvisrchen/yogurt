import { readFileSync } from "node:fs";
import { finish_enhance, initSync, preview, render_enhance, too_short } from "../wasm/yogurt_wasm.js";

// The cases crates/yogurt-server/tests/wasm_parity.rs runs through the Mac's /enhance route, with the enriched_md it stored.
const cases: { name: string; notes_md: string; transcript_json: string; template: string | null; llm_output: string; enriched_md: string; expected_template: string }[] = JSON.parse(
  readFileSync(new URL("../../../crates/yogurt-wasm/tests/fixture.json", import.meta.url), "utf8"),
);

beforeAll(() => initSync({ module: readFileSync(new URL("../wasm/yogurt_wasm_bg.wasm", import.meta.url)) }));

test.each(cases.map((c) => [c.name, c] as const))("the built WASM stores what the Mac stores: %s", (_, c) => {
  const out = finish_enhance(c.notes_md, c.transcript_json, c.llm_output, c.template);
  expect(out.enriched_md).toBe(c.enriched_md);
  expect(out.template).toBe(c.expected_template);
  out.free();
});

test("the prompt is one user message carrying the notes, the transcript, and the forced format", () => {
  const [c] = cases;
  const messages = JSON.parse(render_enhance(c.notes_md, c.transcript_json, "standup"));
  expect(messages).toHaveLength(1);
  expect(messages[0].role).toBe("user");
  expect(messages[0].content).toContain(c.notes_md.trim());
  expect(messages[0].content).toContain(c.transcript_json);
  expect(messages[0].content).toContain("Note format: Standup");
});

test("an unknown format is refused, as the Mac's 400 does", () => {
  expect(() => render_enhance("", "[]", "haiku")).toThrow(/unknown template "haiku"/);
});

test("the preview hides reasoning and the template line while streaming", () => {
  expect(preview("<think>still thinking")).toBe("");
  expect(preview("<think>x</think>templ")).toBe("");
  expect(preview("template: standup\n\n## Upd")).toBe("## Upd");
});

test("an accidental start and stop is too short", () => {
  expect(too_short("", JSON.stringify([{ ts_ms: 0, channel: "them", text: "hello there" }]))).toBe(true);
  expect(too_short("- one note", "[]")).toBe(false);
});
