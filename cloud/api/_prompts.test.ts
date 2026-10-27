import { readFileSync } from "node:fs";
import { CHAT_SYSTEM } from "./_prompts";

const template = (name: string) => readFileSync(new URL(`../../crates/yogurt-prompts/templates/${name}`, import.meta.url), "utf8");

test("the chat prompt matches the desktop template", () => {
  expect(CHAT_SYSTEM).toBe(template("chat-system.md"));
});
