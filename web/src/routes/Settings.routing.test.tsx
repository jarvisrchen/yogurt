/**
 * Settings routing tests.
 *
 * Tabs must be URL-driven so a browser refresh keeps the user on the
 * same settings surface (audio, general, transcription, model). Clicking
 * a sidebar item pushes the URL rather than mutating local state.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Navigate, Route, Routes } from "react-router";
import { Settings } from "./Settings";
import type { SettingsView } from "../lib/api/settings";

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
        adapter: "http",
        cli_model: "",
      },
    ],
    presets: [],
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
      setSttKey: vi.fn(),
    },
    audioApi: {
      devices: vi.fn().mockResolvedValue([]),
    },
  };
});

function makeQc() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
}

function renderAt(initialEntry: string) {
  const qc = makeQc();
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[initialEntry]}>
        <Routes>
          <Route
            path="/settings"
            element={<Navigate to="/settings/model" replace />}
          />
          <Route path="/settings/:section" element={<Settings />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("Settings routing", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("/settings/model renders the Model section", async () => {
    renderAt("/settings/model");
    expect(
      await screen.findByRole("heading", { name: /^model$/i }),
    ).toBeInTheDocument();
  });

  it("/settings/transcription renders the Transcription section", async () => {
    renderAt("/settings/transcription");
    expect(
      await screen.findByRole("heading", { name: /^transcription$/i }),
    ).toBeInTheDocument();
  });

  it("/settings/audio renders the Audio section", async () => {
    renderAt("/settings/audio");
    expect(
      await screen.findByRole("heading", { name: /^audio$/i }),
    ).toBeInTheDocument();
  });

  it("/settings/general renders the General section", async () => {
    renderAt("/settings/general");
    expect(
      await screen.findByRole("heading", { name: /^general$/i }),
    ).toBeInTheDocument();
  });

  it("clicking a sidebar tab changes the rendered pane", async () => {
    renderAt("/settings/model");
    // Start on Model.
    expect(
      await screen.findByRole("heading", { name: /^model$/i }),
    ).toBeInTheDocument();

    const audioTab = await screen.findByRole("button", { name: /^audio$/i });
    fireEvent.click(audioTab);

    // After the navigate, the Audio heading mounts and Model unmounts.
    expect(
      await screen.findByRole("heading", { name: /^audio$/i }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /^model$/i })).toBeNull();
  });

  it("/settings redirects to /settings/model", () => {
    renderAt("/settings");
    // The Navigate element redirects in-render; asserting the panel
    // content is the same as /settings/model.
    return screen
      .findByRole("heading", { name: /^model$/i })
      .then(() => undefined);
  });

  it("refresh on /settings/transcription keeps the user on the same surface", async () => {
    // refresh = a new render at the same URL. Guard against the bug where
    // a refresh on /settings/transcription bounced the user back to model.
    const first = renderAt("/settings/transcription");
    expect(
      await screen.findByRole("heading", { name: /^transcription$/i }),
    ).toBeInTheDocument();
    first.unmount();

    renderAt("/settings/transcription");
    expect(
      await screen.findByRole("heading", { name: /^transcription$/i }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /^model$/i })).toBeNull();
  });
});
