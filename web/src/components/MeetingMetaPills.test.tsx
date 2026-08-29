import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MeetingMetaPills, parseSttEngine } from "./MeetingMetaPills";

afterEach(() => cleanup());

describe("parseSttEngine", () => {
  it("splits provider and model", () => {
    expect(parseSttEngine("local · small.en")).toEqual({
      cloud: false,
      text: "Local · small.en",
    });
    expect(parseSttEngine("cloud · nova-3")).toEqual({ cloud: true, text: "Cloud · nova-3" });
  });
  it("renders a bare provider as '<Provider> STT' and null for missing", () => {
    expect(parseSttEngine("local")).toEqual({ cloud: false, text: "Local STT" });
    expect(parseSttEngine(null)).toBeNull();
    expect(parseSttEngine("  ")).toBeNull();
  });
});

describe("MeetingMetaPills", () => {
  it("renders date, duration, and engine pills", () => {
    const start = new Date(2026, 7, 28, 13, 8).getTime();
    render(
      <MeetingMetaPills
        startedAt={start}
        endedAt={start + 8 * 60_000}
        sttEngine="local · small.en"
      />,
    );
    expect(screen.getByText(/Aug 28/)).toBeInTheDocument();
    expect(screen.getByText("8 min")).toBeInTheDocument();
    expect(screen.getByText("Local · small.en")).toBeInTheDocument();
  });

  it("omits duration while the meeting is live and the engine when unknown", () => {
    const start = Date.now();
    render(<MeetingMetaPills startedAt={start} endedAt={null} sttEngine={null} />);
    expect(screen.queryByText(/min$/)).toBeNull();
    expect(screen.queryByText(/STT|Local|Cloud/)).toBeNull();
  });

  it("renders nothing when there is no metadata at all", () => {
    const { container } = render(<MeetingMetaPills />);
    expect(container).toBeEmptyDOMElement();
  });
});
