import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { PushProgress, describeLastPush, formatPushTime } from "./CloudSection";
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

describe("PushProgress", () => {
  it("shows the count and bar, then the retry countdown", () => {
    const { rerender } = render(<PushProgress push={{ sent: 340, total: 1412, retry_in: null, announced: true }} />);
    expect(screen.getByText("Pushing 340 of 1,412")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "340");
    expect(screen.queryByText(/Retrying/)).not.toBeInTheDocument();
    rerender(<PushProgress push={{ sent: 340, total: 1412, retry_in: 45, announced: true }} />);
    expect(screen.getByText("Retrying in 45s")).toBeInTheDocument();
  });

  it("says it is preparing before the plan is known", () => {
    render(<PushProgress push={{ sent: 0, total: 0, retry_in: null, announced: false }} />);
    expect(screen.getByText("Preparing to push…")).toBeInTheDocument();
  });
});
