import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import type { SettingsView } from "../lib/api/settings";

const { baseFixture } = vi.hoisted(() => {
  const baseFixture: SettingsView = {
    general: {
      port: 7878,
      open_browser_on_start: true,
      audio_input_device: "",
        audio_echo_feature: false,
        audio_echo_output_device: "",
        audio_echo_enabled: false,
        audio_echo_buffer: 512,
      first_run_completed: true,
      stt_provider: "cloud",
      stt_model: "small.en",
      meeting_detection: true,
      meeting_detection_focus: true,
      meeting_auto_stop: true,
      live_recap: true,
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
        adapter: "http",
        cli_model: "",
      },
      {
        id: "01HYYYYYYYYYYYYYYYYYYYYYYY",
        name: "Minimax",
        base_url: "https://api.minimax.io/v1",
        model: "MiniMax-Text-01",
        is_active: false,
        created_at: 1719360000,
        api_key_masked: null,
        adapter: "http",
        cli_model: "",
      },
    ],
    presets: [
      {
        name: "Ollama (local)",
        base_url: "http://localhost:11434/v1",
        default_model: "llama3.1:8b",
        models: ["llama3.1:8b", "mistral"],
        docs_url: "https://ollama.com/library",
        adapter: "http",
        default_cli_model: "",
      },
    ],
    deepgram_key_masked: null,
    paths: {
      data_dir: "~/.yogurt",
      keys: "~/.yogurt/keys.json",
      notes: "~/.yogurt/notes",
    },
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

  it("names the server's own data folder in the sidebar footer", async () => {
    vi.mocked(settingsApi.get).mockResolvedValueOnce({
      ...baseFixture,
      paths: {
        data_dir: "/tmp/scratch",
        keys: "/tmp/scratch/keys.json",
        notes: "/tmp/scratch/notes",
      },
    });
    renderSettings();

    expect(await screen.findByText("/tmp/scratch/keys.json")).toBeInTheDocument();
    expect(screen.getByText("/tmp/scratch/")).toBeInTheDocument();
    expect(screen.queryByText(/\.yogurt/)).toBeNull();
  });

  it("renders the active provider card with masked key + Local-only pill", async () => {
    renderSettings();

    // "Local Ollama" is unique to the active card, unlike the preset chips.
    await waitFor(() => {
      expect(screen.getByText("Local Ollama")).toBeInTheDocument();
    });

    expect(screen.getByText("••••WXYZ")).toBeInTheDocument();

    expect(screen.getByText(/Local-only · on/)).toBeInTheDocument();

    // Sanity: the raw "sk-" prefix never appears anywhere in the rendered tree.
    expect(screen.queryByText(/sk-/)).not.toBeInTheDocument();
  });
});

describe("inactive provider key entry", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("reveals a key field on the inactive row and saves without activating", async () => {
    renderSettings();

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
    expect(settingsApi.activateProvider).not.toHaveBeenCalled();
  });
});

describe("test connection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // clearAllMocks keeps implementations, so restore the baseline fixture
    // for the tests that don't override it.
    vi.mocked(settingsApi.get).mockResolvedValue(baseFixture);
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

  it("tests the stored key from a collapsed provider card", async () => {
    // A fully set-up (stored key, saved model) but failing card sits collapsed, so `Test` must be reachable there.
    vi.mocked(settingsApi.get).mockResolvedValue({
      ...baseFixture,
      providers: baseFixture.providers.map((p) =>
        p.is_active ? p : { ...p, api_key_masked: "••••1234" },
      ),
    });
    vi.mocked(settingsApi.testProvider).mockResolvedValue({
      ok: false,
      error: "LLM call failed: 401 Unauthorized",
    });
    renderSettings();

    const card = (await screen.findAllByTestId("inactive-provider-card"))[0];
    expect(screen.queryByLabelText("API key for Minimax")).toBeNull();

    fireEvent.click(
      within(card).getByRole("button", { name: "Test connection for Minimax" }),
    );

    await waitFor(() => {
      expect(settingsApi.testProvider).toHaveBeenCalledWith(
        "01HYYYYYYYYYYYYYYYYYYYYYYY",
        undefined,
      );
    });
    expect(await screen.findByRole("status")).toHaveTextContent(
      /401 Unauthorized/,
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

describe("API key live preview", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the last 4 chars as ••••XXXX once the draft is long enough", async () => {
    renderSettings();

    fireEvent.click(await screen.findByRole("button", { name: "Add key" }));
    const field = await screen.findByLabelText("API key for Minimax");

    expect(screen.queryByText(/Will save as/)).not.toBeInTheDocument();

    fireEvent.change(field, { target: { value: "sk-ABCDefgh" } });
    expect(
      await screen.findByText(/Will save as/),
    ).toHaveTextContent("••••efgh");
  });
});

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
      adapter: "http" as const,
      cli_model: "",
    };

    // The factory's `get` returns a static fixture, so the cloned row would never appear after
    // invalidation. Mock `get` to append providers `createProvider` has returned.
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

    // waitFor, not findAllBy*, which resolves on the first hit before the refetch re-renders.
    await waitFor(() => {
      expect(screen.getAllByTestId("inactive-provider-card")).toHaveLength(2);
    });

    // The freshly cloned card has its key input open (it is not in the DOM until keying=true).
    const input = await screen.findByLabelText("API key for Google Gemini");
    expect(input).toBeInTheDocument();

    expect(
      screen.getByRole("button", { name: "Add key" }),
    ).toBeInTheDocument();
  });
});

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

    // Scope to the inactive card testid: a `+ Add` chip on the page shares the accessible name.
    const minimaxCard = (
      await screen.findAllByTestId("inactive-provider-card")
    )[0];
    fireEvent.click(
      within(minimaxCard).getByRole("button", { name: "Add key" }),
    );

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

describe("preset docs link", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders a See all {preset} models link anchored at the preset's docs_url", async () => {
    renderSettings();

    // Scope to the active card: the inactive Minimax row matches no preset base_url, so it has no docs link.
    const activeCard = await screen.findByTestId("active-provider-card");
    // The link text uses the preset's brand name, not the user's provider nickname.
    const link = await within(activeCard).findByRole("link", {
      name: /see all ollama \(local\) models/i,
    });
    expect(link).toHaveAttribute("href", "https://ollama.com/library");
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
  });
});
