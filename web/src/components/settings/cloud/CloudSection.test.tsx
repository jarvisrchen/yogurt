import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { LastPushLine, PushProgress, describeEntry, describeLastPush, formatPushTime } from "./CloudSection";
import type { PushHistoryEntry } from "../../../lib/api/cloud";
import { QrCode } from "./QrCode";

const AT = new Date(2026, 9, 23, 14, 59).getTime();

describe("describeLastPush", () => {
  it("formats the time in the mock's short style", () => {
    expect(formatPushTime(AT)).toBe("10/23/26 2:59 PM");
  });

  it("covers never and success", () => {
    expect(describeLastPush(null)).toBe("Not pushed yet");
    expect(describeLastPush({ at: AT, changed: 1, deleted: 0 })).toBe(
      "Last push 10/23/26 2:59 PM · 1 uploaded",
    );
    expect(describeLastPush({ at: AT, changed: 0, deleted: 0 })).toBe("Last push 10/23/26 2:59 PM · up to date");
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

const pushed = (at: number, changed: number, deleted = 0): PushHistoryEntry => ({
  at,
  trigger: "auto",
  outcome: "pushed",
  changed,
  deleted,
});
const failed = (at: number, needs_you = false): PushHistoryEntry => ({
  at,
  trigger: "manual",
  outcome: "failed",
  error: needs_you ? "Sign in again." : "The cloud is unreachable.",
  code: needs_you ? "session_expired" : "network",
  needs_you,
});

describe("describeEntry", () => {
  it("names only the counts that happened", () => {
    expect(describeEntry(pushed(AT, 4, 1))).toBe("4 uploaded, 1 deleted");
    expect(describeEntry(pushed(AT, 0, 2))).toBe("2 deleted");
    expect(describeEntry({ ...pushed(AT, 0), outcome: "up_to_date" })).toBe("Up to date");
    expect(describeEntry(failed(AT))).toBe("Failed: The cloud is unreachable.");
  });
});

describe("LastPushLine", () => {
  const last = { at: AT, changed: 4, deleted: 0 };

  it("hides the toggle with no history and stays muted when healthy", () => {
    const { rerender } = render(<LastPushLine last={null} history={[]} />);
    expect(screen.queryByRole("button", { name: /history/i })).not.toBeInTheDocument();
    rerender(<LastPushLine last={last} history={[pushed(AT, 4)]} />);
    expect(screen.queryByText(/failed/)).not.toBeInTheDocument();
  });

  it("counts failures since the last success and lists every run when opened", () => {
    render(<LastPushLine last={last} history={[failed(AT + 2), failed(AT + 1), pushed(AT, 4), failed(AT - 1)]} />);
    expect(screen.getByText(/· 2 failed since/)).toBeInTheDocument();
    const toggle = screen.getByRole("button", { name: /history/i });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(4);
    expect(rows[0]).toHaveTextContent("Manual");
    expect(rows[2]).toHaveTextContent("Auto4 uploaded");
  });

  it("drops 'since' before any success", () => {
    render(<LastPushLine last={null} history={[failed(AT, true)]} />);
    expect(screen.getByText(/· 1 failed$/)).toBeInTheDocument();
  });
});
