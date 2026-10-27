import { createServer, type Server } from "node:http";
import { expect, test } from "@playwright/test";
import { DEVICES, LATE, expectScreen, isPhone, onboard, reset, revokeBrowsers, seed, shot } from "./helpers";

// A stand-in for the LLM provider at the address playwright.config.ts gives the webapp. It streams a fixed answer and keeps each request.
const LLM_PORT = 5192;
const ANSWER = ["You agreed to sign ", "the vendor contract ", "on **Friday**."];
let llm: Server;
const asked: { model: string; messages: { role: string; content: string }[] }[] = [];

test.describe.configure({ mode: "serial" });
test.beforeAll(async () => {
  await reset();
  llm = createServer(async (req, res) => {
    let body = "";
    for await (const c of req) body += c;
    asked.push(JSON.parse(body));
    res.writeHead(200, { "content-type": "text/event-stream" });
    for (const content of ANSWER) {
      res.write(`data: ${JSON.stringify({ choices: [{ delta: { content }, finish_reason: null }] })}\n\n`);
      await new Promise((r) => setTimeout(r, 150));
    }
    res.end("data: [DONE]\n\n");
  });
  await new Promise<void>((r) => llm.listen(LLM_PORT, "127.0.0.1", r));
});
test.afterAll(() => new Promise<void>((r) => (llm ? llm.close(() => r()) : r())));

test("ask a meeting about its transcript", async ({ page }, info) => {
  const phone = isPhone(page);
  await onboard(page);
  await seed();
  await page.reload();
  await page.getByRole("button", { name: /Q4 planning sync/ }).click();
  await expect(page.getByRole("heading", { name: "Q4 planning sync" })).toBeVisible();

  const pill = page.getByRole("button", { name: "Ask this meeting" });
  await expect(pill).toBeInViewport();
  await shot(page, info, phone ? "u-meeting-ask" : "u-desktop-meeting-ask");

  await pill.click();
  const chat = page.getByRole("region", { name: "Ask the meeting chat" });
  await expect(chat.getByText("Ask anything about what's been said so far.")).toBeVisible();
  await shot(page, info, phone ? "u-chat-empty" : "u-desktop-chat-empty");

  const before = asked.length;
  await chat.getByPlaceholder("Ask this meeting…").fill("When do we sign the vendor contract?");
  await chat.getByPlaceholder("Ask this meeting…").press("Enter");
  await expect(chat.getByText("When do we sign the vendor contract?")).toBeVisible();
  await expect(chat.getByText("Friday", { exact: true })).toBeVisible();
  await expect(chat.getByText(/You agreed to sign the vendor contract on/)).toBeVisible();
  await shot(page, info, phone ? "u-chat-answer" : "u-desktop-chat-answer");

  // The function sent the meeting's transcript, read from the Cloud, with the question last.
  const sent = asked[before];
  expect(sent.model).toBe("fake-model");
  expect(sent.messages.map((m) => m.role)).toEqual(["system", "system", "user"]);
  expect(sent.messages[1].content).toContain(`[10:00] me: ${LATE.text}`);
  expect(sent.messages[2].content).toBe("When do we sign the vendor contract?");

  // A second question carries the first turn along.
  await chat.getByPlaceholder("Ask this meeting…").fill("Who owns the migration plan?");
  await chat.getByPlaceholder("Ask this meeting…").press("Enter");
  await expect.poll(() => asked.length).toBe(before + 2);
  expect(asked[before + 1].messages.map((m) => m.role)).toEqual(["system", "system", "user", "assistant", "user"]);
  expect(asked[before + 1].messages[3].content).toBe(ANSWER.join(""));

  // Crossing the phone and desktop breakpoint, as rotating a phone does, keeps the conversation.
  const size = page.viewportSize()!;
  await page.setViewportSize(phone ? { width: 844, height: 390 } : { width: 390, height: 844 });
  await expect(chat.getByText("When do we sign the vendor contract?")).toBeVisible();
  await page.setViewportSize(size);
  await expect(chat.getByText("When do we sign the vendor contract?")).toBeVisible();

  // A transcript link folds the chat away, so the line it scrolls to is in view.
  await page.getByRole("tab", { name: "Enhanced" }).click();
  await page.getByRole("tabpanel").locator("[data-transcript-link]").click();
  await expect(pill).toBeVisible();
  await expect(page.locator("[data-transcript-ts-sec]", { hasText: LATE.text })).toBeInViewport({ ratio: 1 });
  await pill.click();

  // Escape folds the chat back into the pill, and leaving the meeting forgets the conversation.
  await page.keyboard.press("Escape");
  await expect(pill).toBeVisible();
  await pill.click();
  await expect(chat.getByText("When do we sign the vendor contract?")).toBeVisible();
  await page.goto(`/#/m/${DEVICES.a}/m-standup`);
  await expect(page.getByRole("heading", { name: /edis update/ })).toBeVisible();
  await page.goto(`/#/m/${DEVICES.b}/m-planning`);
  await page.getByRole("button", { name: "Ask this meeting" }).click();
  await expect(chat.getByText("Ask anything about what's been said so far.")).toBeVisible();

  // A meeting opened from an earlier push has no chat.
  await page.goto(`/#/at/2/m/${DEVICES.b}/m-planning`);
  await expect(page.getByText("Steven owns the migration plan")).toBeVisible();
  await expect(page.getByRole("button", { name: "Ask this meeting" })).toHaveCount(0);

  // A browser locked from elsewhere goes to Unlock, and the LLM never sees the question.
  await page.goto(`/#/m/${DEVICES.b}/m-planning`);
  await page.getByRole("button", { name: "Ask this meeting" }).click();
  await revokeBrowsers();
  const asks = asked.length;
  await chat.getByPlaceholder("Ask this meeting…").fill("What did we decide?");
  await chat.getByPlaceholder("Ask this meeting…").press("Enter");
  await expectScreen(page, "Unlock your meetings");
  expect(asked.length).toBe(asks);
});
