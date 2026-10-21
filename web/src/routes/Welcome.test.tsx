import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Welcome } from "./Welcome";

const state = vi.hoisted(() => ({
  settings: undefined as unknown,
  models: undefined as unknown,
}));

vi.mock("../lib/api/settings", () => ({
  useSettings: () => state.settings,
  useSetFirstRunCompleted: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));
vi.mock("../lib/api/stt", () => ({ useModels: () => state.models }));
vi.mock("../hooks/useScreenRecordingStatus", () => ({
  useScreenRecordingStatus: () => ({ granted: true }),
}));
vi.mock("../hooks/useMicrophoneStatus", () => ({
  useMicrophoneStatus: () => ({ granted: true, status: "granted" }),
}));
vi.mock("../lib/api/audio", () => ({
  permissionsKey: ["permissions"],
  requestMicrophonePermission: vi.fn(),
  requestScreenRecordingPermission: vi.fn(),
}));
vi.mock("../components/Logo", () => ({ Logo: () => null }));

function setup(
  general: { stt_provider: string; stt_model: string },
  opts: { deepgramKey?: string | null; downloaded?: boolean } = {},
) {
  state.settings = {
    data: {
      general,
      providers: [{ is_active: true, name: "OpenAI" }],
      deepgram_key_masked: opts.deepgramKey ?? null,
    },
  };
  state.models = {
    data: [{ name: "large-v3-turbo", downloaded: !!opts.downloaded }],
  };
  render(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter>
        <Welcome />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return screen.getByText("Pick transcription").closest("[data-state]")!;
}

describe("Welcome step 4", () => {
  it("is done for local whisper with the model downloaded", () => {
    const card = setup(
      { stt_provider: "local", stt_model: "large-v3-turbo" },
      { downloaded: true },
    );
    expect(card.getAttribute("data-state")).toBe("done");
    expect(screen.queryByText(/Choose in Settings/)).toBeNull();
  });

  it("is current for local whisper with the model missing", () => {
    const card = setup({ stt_provider: "local", stt_model: "large-v3-turbo" });
    expect(card.getAttribute("data-state")).toBe("current");
    expect(
      screen.getByText(/Choose in Settings/).closest("a")?.getAttribute("href"),
    ).toBe("/settings/transcription");
  });

  it("is done for cloud with a stored Deepgram key", () => {
    const card = setup(
      { stt_provider: "cloud", stt_model: "" },
      { deepgramKey: "••••abcd" },
    );
    expect(card.getAttribute("data-state")).toBe("done");
  });

  it("is current for cloud without a key", () => {
    const card = setup({ stt_provider: "cloud", stt_model: "" });
    expect(card.getAttribute("data-state")).toBe("current");
  });
});
