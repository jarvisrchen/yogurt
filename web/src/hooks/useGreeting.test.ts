import { describe, expect, it } from "vitest";
import { greetingFor } from "./useGreeting";

describe("greetingFor", () => {
  it("returns 'morning' before noon", () => {
    const at = new Date();
    at.setHours(9, 0, 0, 0);
    expect(greetingFor(at).timeOfDay).toBe("morning");
    expect(greetingFor(at).greeting).toBe("Good morning");
  });

  it("returns 'afternoon' between 12 and 18", () => {
    const at = new Date();
    at.setHours(14, 30, 0, 0);
    expect(greetingFor(at).timeOfDay).toBe("afternoon");
    expect(greetingFor(at).greeting).toBe("Good afternoon");
  });

  it("returns 'evening' from 18 onward", () => {
    const at = new Date();
    at.setHours(21, 0, 0, 0);
    expect(greetingFor(at).timeOfDay).toBe("evening");
    expect(greetingFor(at).greeting).toBe("Good evening");
  });

  it("honors nameOverride when provided", () => {
    const at = new Date();
    at.setHours(7, 0, 0, 0);
    const g = greetingFor(at, "Jarvis");
    expect(g.name).toBe("Jarvis");
    expect(g.greeting).toBe("Good morning, Jarvis");
  });
});
