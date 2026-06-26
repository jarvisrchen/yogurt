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
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { SettingsView } from "../lib/api/settings";

// ─── Mock the typed API client before importing the component ──────────────
vi.mock("../lib/api/settings", () => {
  const fixture: SettingsView = {
    general: {
      port: 7878,
      open_browser_on_start: true,
      audio_input_device: "",
      first_run_completed: true,
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
    },
    audioApi: {
      devices: vi.fn().mockResolvedValue([]),
    },
  };
});

// Import AFTER vi.mock so the mocked module is wired up
import { Settings } from "./Settings";

function renderSettings() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <Settings />
    </QueryClientProvider>,
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
