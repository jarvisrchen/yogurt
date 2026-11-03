import { describe, expect, it } from "vitest";
import { exportStem } from "./meetings";

// Same cases as export_file_name's test in crates/yogurt-cli/src/commands/ctl/meeting.rs.
describe("exportStem", () => {
  it("names the file the way ctl meeting export does", () => {
    expect(exportStem("Q3 / Q4: plan?")).toBe("Q3 Q4 plan");
    expect(exportStem("  ")).toBe("");
    expect(exportStem(".plan\u0000review")).toBe("plan review");
    expect(exportStem("会".repeat(200))).toBe("会".repeat(80));
  });
});
