// Chat across meetings: ports of crates/yogurt-server/src/api/library_chat.rs and yogurt_notes::ast::strip_markers.
// Dates are in the asker's time zone, which the webapp sends, where the desktop uses the Mac's.
import { LIBRARY_CHAT_SYSTEM } from "./_prompts.js";

/** Earlier turns sent back to the model. Retrieval only ever looks at the latest question. */
const HISTORY_TURNS = 6;

export interface Hit {
  device_id: string;
  meeting_id: string;
  title: string;
  started_at: number;
  notes: string;
  excerpt: string;
}

const STOPWORDS = new Set(
  "a about after all also am an and any anything are as at be been before but by can could did do does for from get got had has have he her him his how i if in is it its me meeting meetings my of on or our say said she should so tell than that the their them then there these they this those to us was we were what when where which who why will with would you your today yesterday week month year last past ago".split(" "),
);
const VERB_MONTHS = ["may", "march"];
const MONTH_CUES = ["in", "during", "since", "from", "until", "last", "this", "early", "mid", "late", "of"];
const MONTHS = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];

// Marks stay with their word, so a virama or a vowel sign does not split it as it does on the desktop.
const words = (q: string) => q.split(/[^\p{Alphabetic}\p{M}\p{N}]+/u).filter(Boolean).map((w) => w.toLowerCase());

/** The question's content words, lowercased and deduplicated, for an ORed full-text query. */
export function extractTerms(q: string): string[] {
  const out: string[] = [];
  for (const w of words(q)) if ([...w].length >= 2 && !STOPWORDS.has(w) && !MONTHS.includes(w) && !out.includes(w)) out.push(w);
  return out;
}

/** `YYYY-MM-DD`, with day and month overflow carried the way `Date.UTC` carries it. */
const ymd = (y: number, m: number, d: number) => new Date(Date.UTC(y, m - 1, d)).toISOString().slice(0, 10);

/**
 * `[start, end)` dates for the first date phrase the question names, if any, given today as `YYYY-MM-DD`.
 * A bare month name means its most recent occurrence, so "in December" asked in March is last December.
 */
export function dateRange(q: string, today: string): [string, string] | null {
  const w = words(q);
  const has = (s: string) => w.includes(s);
  const pair = (a: string, b: string) => w.some((x, i) => x === a && w[i + 1] === b);
  const [y, m, d] = today.split("-").map(Number);
  const monday = d - ((new Date(Date.UTC(y, m - 1, d)).getUTCDay() + 6) % 7);

  if (has("today")) return [today, ymd(y, m, d + 1)];
  if (has("yesterday")) return [ymd(y, m, d - 1), today];
  if (pair("this", "week")) return [ymd(y, m, monday), ymd(y, m, monday + 7)];
  if (pair("last", "week") || pair("past", "week")) return [ymd(y, m, monday - 7), ymd(y, m, monday)];
  if (pair("this", "month")) return [ymd(y, m, 1), ymd(y, m + 1, 1)];
  if (pair("last", "month") || pair("past", "month")) return [ymd(y, m - 1, 1), ymd(y, m, 1)];
  if (pair("this", "year")) return [ymd(y, 1, 1), ymd(y + 1, 1, 1)];
  if (pair("last", "year") || pair("past", "year")) return [ymd(y - 1, 1, 1), ymd(y, 1, 1)];
  for (const [i, name] of MONTHS.entries()) {
    for (const [pos, x] of w.entries()) {
      if (x !== name) continue;
      const next = w[pos + 1] ?? "";
      const namedYear = /^\d{4}$/.test(next) ? Number(next) : null;
      const day = Number(/^(\d+)\p{Alphabetic}*$/u.exec(next)?.[1]);
      // "May" and "March" are also verbs, so they name a month only next to a date cue: "in May", "last March", "May 12th", "May 2025".
      if (VERB_MONTHS.includes(name) && namedYear === null && !(day >= 1 && day <= 31) && !MONTH_CUES.includes(w[pos - 1])) continue;
      const year = namedYear ?? (i + 1 <= m ? y : y - 1);
      return [ymd(year, i + 1, 1), ymd(year, i + 2, 1)];
    }
  }
  return null;
}

/** Whether `tz` is a time zone this runtime knows. */
export function knownZone(tz: unknown): tz is string {
  if (typeof tz !== "string" || !tz) return false;
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: tz });
    return true;
  } catch {
    return false;
  }
}

/** The date at `ms` in `tz`, as `YYYY-MM-DD`. */
export const dateIn = (ms: number, tz: string) => new Intl.DateTimeFormat("en-CA", { timeZone: tz, year: "numeric", month: "2-digit", day: "2-digit" }).format(ms);

/** Unix ms of midnight starting `date` in `tz`. */
export function startOfDay(date: string, tz: string): number {
  const fmt = new Intl.DateTimeFormat("en-US", { timeZone: tz, hourCycle: "h23", year: "numeric", month: "numeric", day: "numeric", hour: "numeric", minute: "numeric", second: "numeric" });
  // How far the zone's wall clock is ahead of UTC at `t`.
  const offset = (t: number) => {
    const p = Object.fromEntries(fmt.formatToParts(t).map((x) => [x.type, Number(x.value)]));
    return Date.UTC(p.year, p.month - 1, p.day, p.hour, p.minute, p.second) - t;
  };
  const wall = Date.parse(`${date}T00:00:00Z`);
  // The offset at the first guess can differ from the offset at midnight when a DST change falls between them.
  const t = wall - offset(wall - offset(wall));
  // A day whose midnight a DST change skips starts when the clocks jump.
  return dateIn(t, tz) === date ? t : t + offset(t + 6 * 3_600_000) - offset(t);
}

/**
 * Removes the wire-format spans the enhanced notes carry, leaving other markup intact.
 * A `data-ai-grey` span keeps its own text, and a `data-transcript-link` span goes whole, including its `↳ HH:MM` label.
 */
export function stripMarkers(s: string): string {
  let out = "";
  let i = 0;
  while (i < s.length) {
    if (s.startsWith("<span", i)) {
      const end = s.indexOf(">", i);
      if (end >= 0) {
        const tag = s.slice(i, end + 1);
        i = end + 1;
        if (!tag.includes("data-ai-grey") && !tag.includes("data-transcript-link")) {
          out += tag;
          continue;
        }
        // Only the wire span's own text survives: none of a link, and none inside a nested span.
        const keep = !tag.includes("data-transcript-link");
        let depth = 1;
        while (i < s.length && depth > 0) {
          if (s.startsWith("<span", i)) {
            const e = s.indexOf(">", i);
            if (e < 0) break;
            depth++;
            i = e + 1;
          } else if (s.startsWith("</span>", i)) {
            depth--;
            i += 7;
          } else {
            if (keep && depth === 1) out += s[i];
            i++;
          }
        }
        continue;
      }
    }
    if (s.startsWith("</span>", i)) {
      i += 7;
      continue;
    }
    out += s[i++];
  }
  return out;
}

// A start time past what Date can hold, as a laptop with a bad clock could push, shows no date.
const day = (ms: number, tz: string) => (Number.isFinite(new Date(ms).getTime()) ? dateIn(ms, tz) : "");

type Message = { role: "system" | "user" | "assistant"; content: string };

/** A port of `build_messages`: the prompt with today's date, the numbered meetings, then the recent conversation. */
export function libraryMessages(today: string, tz: string, hits: Hit[], conversation: Message[]): Message[] {
  let context = "MEETINGS:\n";
  if (!hits.length) context += "\n(no meetings matched)\n";
  hits.forEach((h, i) => {
    context += `\n[${i + 1}] ${h.title} (${day(h.started_at, tz)})\nNotes:\n${stripMarkers(h.notes).trim()}\n`;
    if (h.excerpt.trim()) context += `Transcript excerpt:\n${h.excerpt.trim()}\n`;
  });
  return [
    { role: "system", content: `${LIBRARY_CHAT_SYSTEM.trim()}\n\nToday is ${today}.` },
    { role: "system", content: context },
    ...conversation.slice(-(HISTORY_TURNS + 1)),
  ];
}
