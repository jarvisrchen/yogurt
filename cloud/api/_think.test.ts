import { ThinkStripper } from "./_think";

// The cases from crates/yogurt-llm/src/thinking.rs.
test("passes normal chunks through", () => {
  const s = new ThinkStripper();
  expect(s.push("Hel")).toBe("Hel");
  expect(s.push("lo")).toBe("lo");
  expect(s.flush()).toBe("");
});

test("handles tags split across chunks", () => {
  const s = new ThinkStripper();
  expect(s.push("<th")).toBe("");
  expect(s.push("ink>reasoning</th")).toBe("");
  expect(s.push("ink>visible")).toBe("visible");
  expect(s.flush()).toBe("");
});

test("handles multibyte reasoning", () => {
  const s = new ThinkStripper();
  expect(s.push("<think>😀123456789")).toBe("");
  expect(s.push("</think>visible")).toBe("visible");
});

test("flushes an incomplete visible tag", () => {
  const s = new ThinkStripper();
  expect(s.push("<th")).toBe("");
  expect(s.flush()).toBe("<th");
});

test("drops leading blocks and whitespace and keeps a later inline tag", () => {
  const s = new ThinkStripper();
  expect(s.push(" <think>first</think>\n<think>second</think>\n\nanswer <think>literal</think>")).toBe("answer <think>literal</think>");
});

test("drops an unterminated leading block", () => {
  const s = new ThinkStripper();
  expect(s.push("<think>never closes")).toBe("");
  expect(s.flush()).toBe("");
});
