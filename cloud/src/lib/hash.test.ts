import { describe, expect, it } from "vitest";
import { format, HOME, parse, type Loc } from "./hash";

const cases: [string, Loc][] = [
  ["", HOME],
  ["#/settings", { ...HOME, view: "settings" }],
  ["#/settings/pin", { ...HOME, view: "change-pin" }],
  ["#/history", { ...HOME, view: "history" }],
  ["#/m/dev-1/01ABC", { ...HOME, meeting: "dev-1/01ABC" }],
  ["#/at/12", { ...HOME, asOf: 12 }],
  ["#/at/12/m/dev-1/01ABC", { view: "library", asOf: 12, meeting: "dev-1/01ABC" }],
  ["#/m/dev-1/a%2Fb%20c", { ...HOME, meeting: "dev-1/a/b c" }],
];

describe("location hash", () => {
  it.each(cases)("round-trips %s", (hash, loc) => {
    expect(parse(hash)).toEqual(loc);
    expect(format(loc)).toBe(hash);
  });
  it("sends anything unknown home", () => {
    for (const h of ["#/nope", "#/at/x", "#/at/0", "#/m/onlydevice", "#", "#/m/a/%"]) expect(parse(h)).toEqual(HOME);
  });
});
