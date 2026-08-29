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
import { render, screen, waitFor, fireEvent, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import type { SettingsView } from "../lib/api/settings";

// ─── Mock the typed API client before importing the component ──────────────
const { baseFixture } = vi.hoisted(() => {
  const baseFixture: SettingsView = {
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
        models: ["llama3.1:8b", "mistral"],
        docs_url: "https://ollama.com/library",
      },
    ],
    deepgram_key_masked: null,
  };
  return { baseFixture };
});

vi.mock("../lib/api/settings", () => ({
  settingsApi: {
    get: vi.fn().mockResolvedValue(baseFixture),
    patch: vi.fn(),
    createProvider: vi.fn(),
    updateProvider: vi.fn(),
    deleteProvider: vi.fn(),
    activateProvider: vi.fn(),
    setProviderKey: vi.fn(),
    testProvider: vi.fn(),
    setSttKey: vi.fn(),
    listProviderModels: vi.fn(),
  },
  audioApi: {
    devices: vi.fn().mockResolvedValue([]),
  },
}));

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

/**
 * Pasting a key into the password field shows a live preview in the
 * stored-key format (`••••XXXX`, last 4 chars) so the user can confirm
 * they pasted the right key — without it, a partial paste / stray
 * whitespace / wrong-keyboard-layout typo isn't visible until after
 * Save, when the masked view re-renders from the server.
 */
describe("API key live preview", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the last 4 chars as ••••XXXX once the draft is long enough", async () => {
    renderSettings();

    fireEvent.click(await screen.findByRole("button", { name: "Add key" }));
    const field = await screen.findByLabelText("API key for Minimax");

    // Empty draft: no preview yet.
    expect(screen.queryByText(/Will save as/)).not.toBeInTheDocument();

    fireEvent.change(field, { target: { value: "sk-ABCDefgh" } });
    expect(
      await screen.findByText(/Will save as/),
    ).toHaveTextContent("••••efgh");
  });
});

/**
 * Cloning a preset chip should land the user at the API key field with
 * the cursor already there — the whole point of cloning is to paste a key,
 * so requiring a second click on `Add key` would be friction with no upside.
 */
describe("preset cloning auto-opens the key input", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("expands the freshly-cloned provider's key field without an extra click", async () => {
    const cloned = {
      id: "01HZZZZZZZZZZZZZZZZZZZZZZ",
      name: "Google Gemini",
      base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
      model: "gemini-1.5-pro",
      is_active: false,
      created_at: 1719360000,
      api_key_masked: null,
    };

    // The factory's `get` returns a static fixture, so the cloned row
    // would never appear after invalidation. Mock get to a mutable impl
    // that appends any provider createProvider has returned so far — the
    // same shape the real backend would produce.
    const appended: typeof cloned[] = [];
    vi.mocked(settingsApi.createProvider).mockImplementation(async () => {
      appended.push(cloned);
      return cloned;
    });
    vi.mocked(settingsApi.get).mockImplementation(async () => ({
      ...baseFixture,
      providers: [...baseFixture.providers, ...appended],
    }));

    renderSettings();

    fireEvent.click(
      await screen.findByRole("button", { name: /ollama \(local\)/i }),
    );

    // Two inactive cards now — waitFor (not findAllBy*, which resolves on
    // the first hit) so we don't assert before the refetch re-renders.
    await waitFor(() => {
      expect(screen.getAllByTestId("inactive-provider-card")).toHaveLength(2);
    });

    // The freshly-cloned card is the one whose key input is already open
    // — findByLabelText would throw if it were collapsed (the input isn't
    // in the DOM until keying=true).
    const input = await screen.findByLabelText("API key for Google Gemini");
    expect(input).toBeInTheDocument();

    // Pre-existing Minimax card stays collapsed: its "Add key" button is
    // still visible (not replaced by the input).
    expect(
      screen.getByRole("button", { name: "Add key" }),
    ).toBeInTheDocument();
  });
});

/**
 * REGRESSION: when the saved `model` is the only thing wrong with the
 * provider (Google's frequent deprecations are the canonical case), the
 * user must be able to discover what models are actually available
 * BEFORE saving the key — picking a replacement requires knowing the
 * replacement exists.
 *
 * The flow: paste a draft key → the MODEL `Refresh` button becomes
 * enabled → click it → the backend receives the draft key in the body
 * of POST /api/settings/providers/{id}/models. The stored Keychain
 * entry stays untouched.
 */
describe("MODEL Refresh works with a draft API key", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("sends the draft key in the POST body when refreshing without a stored key", async () => {
    vi.mocked(settingsApi.listProviderModels).mockResolvedValue([
      "gemini-2.5-pro",
      "gemini-2.5-flash",
      "gemini-2.0-flash",
    ]);

    renderSettings();

    // Open the key input on the Minimax card (the fixture has no stored
    // key, so the Add key button is showing by default). Scope to the
    // inactive card testid — there is also a `+ Add` chip-button on the
    // page for adding a brand-new provider, which would otherwise collide
    // on the same accessible name.
    const minimaxCard = (
      await screen.findAllByTestId("inactive-provider-card")
    )[0];
    fireEvent.click(
      within(minimaxCard).getByRole("button", { name: "Add key" }),
    );

    // Type a draft key. Refresh should be enabled once any non-empty
    // draft is present, even though no key is stored.
    fireEvent.change(
      await screen.findByLabelText("API key for Minimax"),
      { target: { value: "sk-draft" } },
    );

    const refresh = await screen.findByRole("button", {
      name: /refresh model list for Minimax/i,
    });
    expect(refresh).toBeEnabled();

    fireEvent.click(refresh);

    await waitFor(() => {
      expect(settingsApi.listProviderModels).toHaveBeenCalledWith(
        "01HYYYYYYYYYYYYYYYYYYYYYYY",
        "sk-draft",
      );
    });
  });
});

/**
 * Each preset ships a `docs_url` pointing at the provider's public
 * model catalog. The Settings page renders it as a small link below
 * the MODEL input so the user has a discovery surface even when
 * `/v1/models` is wrong, missing, or behind auth — the canonical
 * case being a Google Gemini deprecation where the user needs to see
 * the new tiers before they can pick one.
 */
describe("preset docs link", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders a See all {preset} models link anchored at the preset's docs_url", async () => {
    renderSettings();

    // Scope to the active card — the inactive Minimax row doesn't match
    // any preset base_url so it has no docs link, which would otherwise
    // either be missing (findBy fails) or be a different match.
    const activeCard = await screen.findByTestId("active-provider-card");
    // The link text uses the preset's brand name ("Ollama (local)"),
    // NOT the user-chosen provider name ("Local Ollama") — the docs
    // page belongs to the brand, not to the user's nickname.
    const link = await within(activeCard).findByRole("link", {
      name: /see all ollama \(local\) models/i,
    });
    expect(link).toHaveAttribute("href", "https://ollama.com/library");
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
  });
});
