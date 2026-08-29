/**
 * Settings page smoke test (Phase 5 Plan 05-04 Task 2).
 *
 * Mocks `../lib/api/settings` so the page renders without a live server.
 * Asserts that:
 *  - The active provider's name + masked key both render in the ProviderCard
 *  - The "Local-only · on" matcha pill renders (because the active provider
 *    uses a localhost base_url — see plan 05-04 task 2 fixture guidance)
 *
 * The fixture intentionally uses Ollama (localhost) as the active provider
 * so the matcha pill renders — Minimax would suppress it because its base
 * URL is api.minimax.io (non-localhost).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import type { SettingsView } from "../lib/api/settings";

// ─── Mock the typed API client before importing the component ──────────────
vi.mock("../lib/api/settings", () => {
  const fixture: SettingsView = {
    general: {
      port: 7878,
      open_browser_on_start: true,
      audio_input_device: "",
      first_run_completed: true,
      stt_provider: "cloud",
      stt_model: "small.en",
    },
    providers: [
      {
        id: "01HXXXXXXXXXXXXXXXXXXXXXXX",
        name: "Local Ollama",
        base_url: "http://localhost:11434/v1",
        model: "llama3.1:8b",
        is_active: true,
        created_at: 1719360000,
        api_key_masked: "••••WXYZ",
      },
      {
        id: "01HYYYYYYYYYYYYYYYYYYYYYYY",
        name: "Minimax",
        base_url: "https://api.minimax.io/v1",
        model: "MiniMax-Text-01",
        is_active: false,
        created_at: 1719360000,
        api_key_masked: null,
      },
    ],
    presets: [
      {
        name: "Ollama (local)",
        base_url: "http://localhost:11434/v1",
        default_model: "llama3.1:8b",
      },
    ],
    deepgram_key_masked: null,
  };
  return {
    settingsApi: {
      get: vi.fn().mockResolvedValue(fixture),
      patch: vi.fn(),
      createProvider: vi.fn(),
      updateProvider: vi.fn(),
      deleteProvider: vi.fn(),
      activateProvider: vi.fn(),
      setProviderKey: vi.fn(),
      testProvider: vi.fn(),
      setSttKey: vi.fn(),
    },
    audioApi: {
      devices: vi.fn().mockResolvedValue([]),
    },
  };
});

// Import AFTER vi.mock so the mocked module is wired up
import { Settings } from "./Settings";
import { settingsApi } from "../lib/api/settings";

function renderSettings() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <MemoryRouter initialEntries={["/settings"]}>
      <QueryClientProvider client={qc}>
        <Settings />
      </QueryClientProvider>
    </MemoryRouter>,
  );
}

describe("Settings page", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the active provider card with masked key + Local-only pill", async () => {
    renderSettings();

    // Wait for the active provider name (proves the query resolved and
    // ProviderCard mounted). The name "Local Ollama" is unique to the
    // active card — distinct from any preset chip.
    await waitFor(() => {
      expect(screen.getByText("Local Ollama")).toBeInTheDocument();
    });

    // Masked key surfaces in the card.
    expect(screen.getByText("••••WXYZ")).toBeInTheDocument();

    // Local-only pill renders because the only active provider has a
    // localhost base_url.
    expect(screen.getByText(/Local-only · on/)).toBeInTheDocument();

    // Sanity: the raw "sk-" prefix never appears anywhere in the rendered tree.
    expect(screen.queryByText(/sk-/)).not.toBeInTheDocument();
  });
});

/**
 * REGRESSION: an inactive provider must be keyable in place.
 *
 * Cloning a preset chip creates an INACTIVE row. Originally only the active
 * `ProviderCard` rendered a key field, so the only route to keying a
 * freshly-cloned provider was `Set active` first — which swaps the live LLM
 * over to a provider that cannot answer yet.
 */
describe("inactive provider key entry", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("reveals a key field on the inactive row and saves without activating", async () => {
    renderSettings();

    // The keyless inactive row (Minimax in the fixture) offers "Add key".
    fireEvent.click(await screen.findByRole("button", { name: "Add key" }));

    const field = await screen.findByLabelText("API key for Minimax");
    fireEvent.change(field, { target: { value: "sk-inactive-provider-key" } });
    fireEvent.click(
      screen.getByRole("button", { name: "Save API key for Minimax" }),
    );

    await waitFor(() => {
      expect(settingsApi.setProviderKey).toHaveBeenCalledWith(
        "01HYYYYYYYYYYYYYYYYYYYYYYY",
        "sk-inactive-provider-key",
      );
    });
    // Keying must NOT have promoted the provider.
    expect(settingsApi.activateProvider).not.toHaveBeenCalled();
  });
});

/**
 * The point of "Test" is a verdict BEFORE the key is committed, so the
 * failing path must report the provider's reason and must not have written
 * anything.
 */
describe("test connection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("reports a rejected key and does not save it", async () => {
    vi.mocked(settingsApi.testProvider).mockResolvedValue({
      ok: false,
      error: "LLM call failed: 401 Unauthorized — Incorrect API key provided",
    });
    renderSettings();

    fireEvent.click(await screen.findByRole("button", { name: "Add key" }));
    fireEvent.change(await screen.findByLabelText("API key for Minimax"), {
      target: { value: "sk-wrong" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Test connection for Minimax" }),
    );

    await waitFor(() => {
      expect(settingsApi.testProvider).toHaveBeenCalledWith(
        "01HYYYYYYYYYYYYYYYYYYYYYYY",
        "sk-wrong",
      );
    });
    expect(await screen.findByRole("status")).toHaveTextContent(
      /401 Unauthorized/,
    );
    // Testing is not saving.
    expect(settingsApi.setProviderKey).not.toHaveBeenCalled();
  });

  it("reports a working key with the model the provider echoed back", async () => {
    vi.mocked(settingsApi.testProvider).mockResolvedValue({
      ok: true,
      model: "MiniMax-Text-01",
    });
    renderSettings();

    fireEvent.click(await screen.findByRole("button", { name: "Add key" }));
    fireEvent.change(await screen.findByLabelText("API key for Minimax"), {
      target: { value: "sk-right" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Test connection for Minimax" }),
    );

    expect(await screen.findByRole("status")).toHaveTextContent(
      /Connection works.*MiniMax-Text-01/,
    );
  });

  it("clears a stale verdict once the key is edited again", async () => {
    vi.mocked(settingsApi.testProvider).mockResolvedValue({
      ok: true,
      model: "MiniMax-Text-01",
    });
    renderSettings();

    fireEvent.click(await screen.findByRole("button", { name: "Add key" }));
    const field = await screen.findByLabelText("API key for Minimax");
    fireEvent.change(field, { target: { value: "sk-right" } });
    fireEvent.click(
      screen.getByRole("button", { name: "Test connection for Minimax" }),
    );
    await screen.findByRole("status");

    // A green tick sitting next to a key that is no longer in the box is a lie.
    fireEvent.change(field, { target: { value: "sk-different" } });
    await waitFor(() => {
      expect(screen.queryByRole("status")).not.toBeInTheDocument();
    });
  });
});
