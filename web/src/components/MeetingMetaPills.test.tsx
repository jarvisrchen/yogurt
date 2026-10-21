import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup, within } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MeetingMetaPills, parseLlmModel, parseSttEngine } from "./MeetingMetaPills";

afterEach(() => cleanup());

describe("parseSttEngine", () => {
  it("labels with the model alone and keeps the provider for the hover", () => {
    expect(parseSttEngine("local · small.en")).toEqual({
      cloud: false,
      text: "small.en",
      full: "Local · small.en",
    });
    expect(parseSttEngine("cloud · nova-3")).toEqual({
      cloud: true,
      text: "nova-3",
      full: "Cloud · nova-3",
    });
  });
  it("renders a bare provider as '<Provider> STT' and null for missing", () => {
    expect(parseSttEngine("local")).toEqual({ cloud: false, text: "Local STT", full: "Local STT" });
    expect(parseSttEngine(null)).toBeNull();
    expect(parseSttEngine("  ")).toBeNull();
  });
});

describe("parseLlmModel", () => {
  it("drops the CLI binary and the provider prefix from the label", () => {
    expect(parseLlmModel("cli:opencode:minimax-coding-plan/MiniMax-M3")).toEqual({
      cli: true,
      text: "MiniMax-M3",
      full: "cli:opencode:minimax-coding-plan/MiniMax-M3",
    });
    expect(parseLlmModel("cli:claude:haiku")?.text).toBe("haiku");
    expect(parseLlmModel("cli:claude")?.text).toBe("claude");
  });
  it("leaves a plain API model as is and null for missing", () => {
    expect(parseLlmModel("gemini-3.5-flash-lite")).toEqual({
      cli: false,
      text: "gemini-3.5-flash-lite",
      full: "gemini-3.5-flash-lite",
    });
    expect(parseLlmModel("  ")).toBeNull();
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
    expect(screen.getByTestId("engine-pill")).toHaveTextContent(/^small\.en/);
    expect(within(screen.getByTestId("engine-pill")).getByRole("tooltip")).toHaveTextContent(
      "Local small.en",
    );
  });

  it("shows the short CLI model and the full stamp on hover", () => {
    render(
      <MeetingMetaPills
        startedAt={Date.now()}
        llmModel="cli:opencode:minimax-coding-plan/MiniMax-M3"
      />,
    );
    const pill = screen.getByTestId("llm-pill");
    expect(within(pill).getByText("MiniMax-M3")).toBeInTheDocument();
    expect(within(pill).getByRole("tooltip")).toHaveTextContent(
      "Enhanced by cli:opencode:minimax-coding-plan/MiniMax-M3",
    );
  });

  it("omits duration while the meeting is live and the engine when unknown", () => {
    const start = Date.now();
    render(<MeetingMetaPills startedAt={start} endedAt={null} sttEngine={null} />);
    expect(screen.queryByText(/min$/)).toBeNull();
    expect(screen.queryByText(/STT|Local|Cloud/)).toBeNull();
  });

  it("renders the LLM pill only once the row carries llm_model", () => {
    const start = Date.now();
    const { rerender } = render(<MeetingMetaPills startedAt={start} llmModel={null} />);
    expect(screen.queryByText("gpt-5-mini")).toBeNull();
    rerender(<MeetingMetaPills startedAt={start} llmModel="gpt-5-mini" />);
    expect(screen.getByTestId("llm-pill")).toHaveTextContent(/^gpt-5-mini/);
  });

  it("renders nothing when there is no metadata at all", () => {
    const { container } = render(<MeetingMetaPills />);
    expect(container).toBeEmptyDOMElement();
  });

  it("paints cloud STT as a neutral card pill, not filled matcha", () => {
    const start = Date.now();
    render(
      <MeetingMetaPills
        startedAt={start}
        endedAt={null}
        sttEngine="cloud · nova-3"
      />,
    );
    const pill = screen.getByTestId("engine-pill");
    expect(pill).toHaveClass("text-mut");
    expect(pill).not.toHaveClass("bg-mtsoft");
    expect(pill).not.toHaveClass("text-ink");
    expect(pill.className).toMatch(/border/);
  });

  it("paints LLM pill as filled blueberry once enhanced", () => {
    const start = Date.now();
    render(<MeetingMetaPills startedAt={start} llmModel="gpt-5-mini" />);
    const pill = screen.getByTestId("llm-pill");
    expect(pill).toHaveClass("bg-blsoft");
    expect(pill).toHaveClass("text-ink");
    expect(pill.className).not.toMatch(/border/);
  });
});
