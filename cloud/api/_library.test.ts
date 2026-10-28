import { dateIn, dateRange, extractTerms, libraryMessages, startOfDay, stripMarkers } from "./_library";
import { LIBRARY_CHAT_SYSTEM } from "./_prompts";

// The cases from crates/yogurt-server/src/api/library_chat.rs.
test("terms drop stopwords, dates, and duplicates", () => {
  expect(extractTerms("What did Steven and I decide about pricing last month? Pricing!")).toEqual(["steven", "decide", "pricing"]);
  expect(extractTerms("what did we say in September?")).toEqual([]);
  expect(extractTerms("प्रोजेक्ट मिटिंग")).toEqual(["प्रोजेक्ट", "मिटिंग"]);
});

test("date phrases resolve against today", () => {
  // 2026-10-21 is a Wednesday.
  const today = "2026-10-21";
  expect(dateRange("pricing last month", today)).toEqual(["2026-09-01", "2026-10-01"]);
  expect(dateRange("this week", today)).toEqual(["2026-10-19", "2026-10-26"]);
  expect(dateRange("last week", today)).toEqual(["2026-10-12", "2026-10-19"]);
  expect(dateRange("yesterday", today)).toEqual(["2026-10-20", today]);
  expect(dateRange("in September", today)).toEqual(["2026-09-01", "2026-10-01"]);
  expect(dateRange("in december", today)).toEqual(["2025-12-01", "2026-01-01"]);
  expect(dateRange("december 2026", today)).toEqual(["2026-12-01", "2027-01-01"]);
  expect(dateRange("last month", "2026-01-05")).toEqual(["2025-12-01", "2026-01-01"]);
  expect(dateRange("what did steven say", today)).toBeNull();
  expect(dateRange("did steven say we may slip?", today)).toBeNull();
  expect(dateRange("what happened in may", today)).toEqual(["2026-05-01", "2026-06-01"]);
  expect(dateRange("may 2025", today)).toEqual(["2025-05-01", "2025-06-01"]);
});

test("a week that crosses a month and a year boundary", () => {
  // 2027-01-01 is a Friday.
  expect(dateRange("this week", "2027-01-01")).toEqual(["2026-12-28", "2027-01-04"]);
  expect(dateRange("today", "2026-12-31")).toEqual(["2026-12-31", "2027-01-01"]);
});

test("days start at the asker's midnight, across a DST change", () => {
  expect(startOfDay("2026-10-27", "America/Los_Angeles")).toBe(Date.parse("2026-10-27T07:00:00Z"));
  expect(startOfDay("2026-11-02", "America/Los_Angeles")).toBe(Date.parse("2026-11-02T08:00:00Z"));
  expect(startOfDay("2026-03-30", "Asia/Kolkata")).toBe(Date.parse("2026-03-29T18:30:00Z"));
  expect(dateIn(Date.parse("2026-10-27T06:59:00Z"), "America/Los_Angeles")).toBe("2026-10-26");
  // Santiago's clocks jump from 00:00 to 01:00 that night, so the day starts at 01:00.
  expect(startOfDay("2026-09-06", "America/Santiago")).toBe(Date.parse("2026-09-06T04:00:00Z"));
});

// The cases from crates/yogurt-notes/src/ast.rs.
test("strip markers keeps non-ASCII text", () => {
  const input = "- Today is the user’s last day <span data-ai-grey data-ts=\"4\">café … 会議 <span data-transcript-link data-ts=\"4\">↳ 00:04</span></span> done";
  expect(stripMarkers(input)).toBe("- Today is the user’s last day café … 会議  done");
});

test("strip markers handles nested wire-format spans", () => {
  expect(stripMarkers('<span data-ai-grey data-ts="120">pricing <span data-transcript-link data-ts="120">↳ 02:00</span></span>')).toBe("pricing ");
  expect(stripMarkers('<span class="x">kept</span> plain')).toBe('<span class="x">kept plain');
});

test("messages number each meeting with its notes and excerpt, then the recent conversation", () => {
  const hit = (n: number, extra = {}) => ({ device_id: "d", meeting_id: `m${n}`, title: `Meeting ${n}`, started_at: Date.parse("2026-10-20T18:00:00Z"), notes: "", excerpt: "", ...extra });
  const turns = Array.from({ length: 9 }, (_, i) => ({ role: i % 2 ? "assistant" : "user", content: `t${i}` }) as const);
  const out = libraryMessages("2026-10-27", "America/Los_Angeles", [hit(1, { notes: ' <span data-ai-grey="">Ship</span> ', excerpt: " we ship friday " }), hit(2)], turns);
  expect(out[0]).toEqual({ role: "system", content: `${LIBRARY_CHAT_SYSTEM.trim()}\n\nToday is 2026-10-27.` });
  expect(out[1].content).toBe("MEETINGS:\n\n[1] Meeting 1 (2026-10-20)\nNotes:\nShip\nTranscript excerpt:\nwe ship friday\n\n[2] Meeting 2 (2026-10-20)\nNotes:\n\n");
  expect(out.slice(2).map((m) => m.content)).toEqual(["t2", "t3", "t4", "t5", "t6", "t7", "t8"]);
  expect(libraryMessages("2026-10-27", "UTC", [], turns)[1].content).toBe("MEETINGS:\n\n(no meetings matched)\n");
  expect(libraryMessages("2026-10-27", "UTC", [hit(1, { started_at: 1.76e18 })], turns)[1].content).toContain("[1] Meeting 1 ()");
});
