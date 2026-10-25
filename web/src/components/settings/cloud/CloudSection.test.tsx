import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { describeLastPush, formatPushTime } from "./CloudSection";
import { QrCode } from "./QrCode";

const AT = new Date(2026, 9, 23, 14, 59).getTime();

describe("describeLastPush", () => {
  it("formats the time in the mock's short style", () => {
    expect(formatPushTime(AT)).toBe("10/23/26 2:59 PM");
  });

  it("covers never and success", () => {
    expect(describeLastPush(null)).toBe("Not pushed yet");
    expect(describeLastPush({ at: AT, changed: 1, deleted: 0 })).toBe(
      "Last push 10/23/26 2:59 PM · 1 uploaded, 0 deleted",
    );
  });
});

describe("QrCode", () => {
  it("draws modules for the address and labels the image", () => {
    render(<QrCode text="https://richard-yogurt.vercel.app" label="QR code for the address" />);
    const svg = screen.getByRole("img", { name: "QR code for the address" });
    expect(svg.getAttribute("viewBox")).toMatch(/^0 0 \d+ \d+$/);
    expect(svg.querySelector("path")?.getAttribute("d")?.length).toBeGreaterThan(100);
  });
});
