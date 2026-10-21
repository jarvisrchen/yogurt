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
  it.each([
    ["local", { downloaded: true }, "done"],
    ["local", {}, "current"],
    ["cloud", { deepgramKey: "••••abcd" }, "done"],
    ["cloud", {}, "current"],
  ] as const)("%s %o is %s", (stt_provider, opts, expected) => {
    const card = setup({ stt_provider, stt_model: "large-v3-turbo" }, opts);
    expect(card.getAttribute("data-state")).toBe(expected);
    const link = screen.queryByText(/Choose in Settings/)?.closest("a");
    expect(link?.getAttribute("href") ?? null).toBe(
      expected === "current" ? "/settings/transcription" : null,
    );
  });
});
