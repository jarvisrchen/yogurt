/**
 * ProviderCard - cli adapter (LLM-4) rendering coverage.
 *
 * The http-adapter branch already has indirect regression coverage via
 * `Settings.test.tsx` (its fixture's active provider is http-adapter
 * Ollama). This file covers the new cli-adapter branch specifically: no
 * BASE URL / MODEL fields, no Edit button (nothing editable), no API KEY
 * section, and a Test button reachable with no key at all.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ProviderView } from "../../lib/api/settings";

const settingsApiMock = vi.hoisted(() => ({
  updateProvider: vi.fn(),
  testProvider: vi.fn(),
}));

vi.mock("../../lib/api/settings", () => ({
  settingsApi: settingsApiMock,
}));

import { ProviderCard } from "./ProviderCard";

const cliProvider: ProviderView = {
  id: "01CLICARD",
  name: "Claude Code (local CLI)",
  base_url: "",
  model: "claude",
  is_active: true,
  created_at: 0,
  api_key_masked: null,
  adapter: "cli",
};

function renderCard() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <ProviderCard provider={cliProvider} />
    </QueryClientProvider>,
  );
}

describe("ProviderCard - cli adapter", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("has no Edit button, no BASE URL / MODEL fields, and no API KEY section", () => {
    renderCard();

    expect(screen.queryByRole("button", { name: "Edit" })).not.toBeInTheDocument();
    expect(screen.queryByText("BASE URL")).not.toBeInTheDocument();
    expect(screen.queryByText("MODEL")).not.toBeInTheDocument();
    expect(screen.queryByText("API KEY · stored locally")).not.toBeInTheDocument();
    expect(screen.getByText("claude")).toBeInTheDocument();
  });

  it("Test is reachable immediately and hits the provider endpoint with no key", async () => {
    settingsApiMock.testProvider.mockResolvedValue({ ok: true, model: "cli:claude" });
    renderCard();

    const testButton = screen.getByRole("button", {
      name: `Test connection for ${cliProvider.name}`,
    });
    expect(testButton).toBeEnabled();

    fireEvent.click(testButton);

    await waitFor(() =>
      expect(settingsApiMock.testProvider).toHaveBeenCalledWith(
        cliProvider.id,
        undefined,
      ),
    );
  });
});
