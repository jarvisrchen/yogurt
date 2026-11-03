import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, within, fireEvent, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  MeetingMetaPills,
  parseLlmModel,
  parseSttEngine,
  providerModelId,
} from "./MeetingMetaPills";
import { settingsApi, settingsKey, type ProviderView } from "../lib/api/settings";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

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

const provider = (p: Partial<ProviderView>): ProviderView => ({
  id: "p",
  name: "P",
  base_url: "",
  model: "",
  is_active: false,
  created_at: 0,
  api_key_masked: null,
  adapter: "http",
  cli_model: "",
  ...p,
});

describe("providerModelId", () => {
  it("matches the llm_model stamp enhance writes", () => {
    expect(providerModelId(provider({ model: "gpt-5.1" }))).toBe("gpt-5.1");
    expect(providerModelId(provider({ adapter: "cli", model: "claude" }))).toBe("cli:claude");
    expect(providerModelId(provider({ adapter: "cli", model: "claude", cli_model: "haiku" }))).toBe(
      "cli:claude:haiku",
    );
  });
});

describe("LlmSwitch", () => {
  function renderLive({ active = "a", ...opts }: { llmLocked?: boolean; active?: string } = {}) {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    qc.setQueryData(settingsKey, {
      providers: [
        provider({ id: "a", name: "OpenAI", model: "gpt-5.1", is_active: active === "a" }),
        provider({ id: "b", name: "Ollama", model: "qwen3:14b", is_active: active === "b" }),
      ],
    });
    vi.spyOn(settingsApi, "get").mockImplementation(async () => qc.getQueryData(settingsKey)!);
    render(
      <QueryClientProvider client={qc}>
        <MemoryRouter>
          <MeetingMetaPills startedAt={Date.now()} llmModel="gpt-5.1" llmPending {...opts} />
        </MemoryRouter>
      </QueryClientProvider>,
    );
  }

  it("activates the picked provider and closes the menu", async () => {
    const activate = vi.spyOn(settingsApi, "activateProvider").mockResolvedValue(provider({}));
    renderLive();
    fireEvent.click(screen.getByTestId("llm-pill"));
    fireEvent.click(screen.getByRole("menuitemradio", { name: /Ollama/ }));
    await waitFor(() => expect(activate).toHaveBeenCalledWith("b"));
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  });

  it("keeps the menu open with the server's error when the switch fails", async () => {
    vi.spyOn(settingsApi, "activateProvider").mockRejectedValue(new Error("provider not found"));
    renderLive();
    fireEvent.click(screen.getByTestId("llm-pill"));
    fireEvent.click(screen.getByRole("menuitemradio", { name: /Ollama/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("provider not found");
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("closes on Escape without letting it reach page shortcuts, and on an outside click", () => {
    renderLive();
    const pageEscape = vi.fn();
    document.addEventListener("keydown", pageEscape);
    fireEvent.click(screen.getByTestId("llm-pill"));
    fireEvent.keyDown(screen.getByRole("menuitemradio", { name: /OpenAI/ }), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(pageEscape).not.toHaveBeenCalled();
    document.removeEventListener("keydown", pageEscape);

    fireEvent.click(screen.getByTestId("llm-pill"));
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens on the checked row and moves focus with the arrow keys", () => {
    renderLive({ active: "b" });
    fireEvent.click(screen.getByTestId("llm-pill"));
    const ollama = screen.getByRole("menuitemradio", { name: /Ollama/ });
    expect(ollama).toHaveFocus();
    fireEvent.keyDown(ollama, { key: "ArrowDown" });
    expect(screen.getByRole("menuitem", { name: "Model settings" })).toHaveFocus();
    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    expect(screen.getByRole("menuitemradio", { name: /OpenAI/ })).toHaveFocus();
    fireEvent.keyDown(document.activeElement!, { key: "ArrowUp" });
    expect(screen.getByRole("menuitem", { name: "Model settings" })).toHaveFocus();
  });

  it("cannot be opened while Enhance runs", () => {
    renderLive({ llmLocked: true });
    expect(screen.getByTestId("llm-pill")).toBeDisabled();
  });
});
