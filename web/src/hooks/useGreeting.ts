/**
 * Phase 7 (Plan 07-01) — greeting hook for the Library hero.
 *
 * Returns a `{ timeOfDay, name, greeting }` triple keyed off the user's
 * local clock. `name` defaults to "you" — PRD §5.9: yogurt doesn't ask
 * for a username in v1, but `nameOverride` lets future surfaces
 * (e.g. a settings field) drive it without rewriting the consumer.
 */

export type TimeOfDay = "morning" | "afternoon" | "evening";

export interface Greeting {
  timeOfDay: TimeOfDay;
  name: string;
  /** Pre-assembled "Good {timeOfDay}, {name}" string. */
  greeting: string;
}

/**
 * Pure-function form so tests can pass in a deterministic clock + override
 * without depending on `Date.now()`.
 */
export function greetingFor(now: Date, nameOverride?: string): Greeting {
  const h = now.getHours();
  const timeOfDay: TimeOfDay =
    h < 12 ? "morning" : h < 18 ? "afternoon" : "evening";
  const name = (nameOverride ?? "").trim() || "you";
  return {
    timeOfDay,
    name,
    greeting: `Good ${timeOfDay}, ${name}`,
  };
}

/**
 * React hook variant. Snapshots the clock at first render — the greeting
 * never flips mid-session even if the user keeps the tab open past the
 * 12pm / 6pm boundary. Re-mounting the Library route refreshes it.
 */
export function useGreeting(nameOverride?: string): Greeting {
  return greetingFor(new Date(), nameOverride);
}
