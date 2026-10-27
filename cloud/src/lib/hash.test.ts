import { describe, expect, it } from "vitest";
import { format, HOME, parse, type Loc } from "./hash";

const cases: [string, Loc][] = [
  ["", HOME],
  ["#/settings", { ...HOME, view: "settings" }],
  ["#/settings/pin", { ...HOME, view: "change-pin" }],
  ["#/history", { ...HOME, view: "history" }],
  ["#/m/dev-1/01ABC", { ...HOME, meeting: "dev-1/01ABC" }],
  ["#/at/12", { ...HOME, asOf: 12 }],
  ["#/at/12/m/dev-1/01ABC", { ...HOME, asOf: 12, meeting: "dev-1/01ABC" }],
  ["#/p/3", { ...HOME, page: 3 }],
  ["#/n/50", { ...HOME, size: 50 }],
  ["#/p/2/n/100", { ...HOME, page: 2, size: 100 }],
  ["#/at/12/p/2/n/10", { ...HOME, asOf: 12, page: 2, size: 10 }],
  ["#/p/2/n/50/m/dev-1/01ABC", { ...HOME, page: 2, size: 50, meeting: "dev-1/01ABC" }],
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
  it("falls back to the first page and the stored size for a bad page or size", () => {
    for (const h of ["#/p/0", "#/p/x", "#/p/-2", "#/p/1.5", "#/n/7", "#/n/x", "#/p"]) expect(parse(h)).toEqual(HOME);
  });
  it("leaves the first page and the default size out of the URL", () => {
    expect(format({ ...HOME, page: 1, size: null })).toBe("");
  });
});
