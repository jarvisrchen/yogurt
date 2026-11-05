import { beforeEach, describe, expect, it } from "vitest";
import { keepSearch, libraryUrl, rememberLibrary } from "./libraryReturn";

beforeEach(() => sessionStorage.clear());

describe("libraryReturn", () => {
  it("returns to the last library view, or All meetings when there is none", () => {
    expect(libraryUrl()).toBe("/");
    rememberLibrary({ url: "/archive/label/L1?q=roof&page=3", y: 480 });
    expect(libraryUrl()).toBe("/archive/label/L1?q=roof&page=3");
  });

  it("carries only the search to another view", () => {
    expect(keepSearch("?q=roof+truss&page=3&size=50")).toBe("?q=roof+truss");
    expect(keepSearch("?page=3")).toBe("");
  });
});
