/**
 * ProviderCard - cli adapter (LLM-4) rendering coverage.
 *
 * The http-adapter branch already has indirect regression coverage via
 * `Settings.test.tsx` (its fixture's active provider is http-adapter
 * Ollama). This file covers the new cli-adapter branch specifically: no
 * BASE URL field, no Edit button (nothing editable there), no API KEY
 * section, a Test button reachable with no key at all, and a `--model`
 * override picker gated behind a successful Test.
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

const freshCliProvider: ProviderView = {
  id: "01CLICARD",
  name: "Claude Code (local CLI)",
  base_url: "",
  model: "claude",
  is_active: true,
  created_at: 0,
  api_key_masked: null,
  adapter: "cli",
  cli_model: "",
};

function renderCard(
  provider: ProviderView = freshCliProvider,
  defaultCliModel = "",
) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <ProviderCard provider={provider} defaultCliModel={defaultCliModel} />
    </QueryClientProvider>,
  );
}

describe("ProviderCard - cli adapter", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("has no Edit button, no BASE URL field, no API KEY section, and no model picker before a Test passes", () => {
    renderCard();

    expect(screen.queryByRole("button", { name: "Edit" })).not.toBeInTheDocument();
    expect(screen.queryByText("BASE URL")).not.toBeInTheDocument();
    expect(screen.queryByText("API KEY · stored locally")).not.toBeInTheDocument();
    expect(screen.getByText("claude")).toBeInTheDocument();
    expect(
      screen.queryByLabelText(`Model for ${freshCliProvider.name}`),
    ).not.toBeInTheDocument();
    expect(screen.getByText(/click test below/i)).toBeInTheDocument();
  });

  it("Test is reachable immediately and hits the provider endpoint with no key", async () => {
    settingsApiMock.testProvider.mockResolvedValue({ ok: true, model: "cli:claude" });
    renderCard();

    const testButton = screen.getByRole("button", {
      name: `Test connection for ${freshCliProvider.name}`,
    });
    expect(testButton).toBeEnabled();

    fireEvent.click(testButton);

    await waitFor(() =>
      expect(settingsApiMock.testProvider).toHaveBeenCalledWith(
        freshCliProvider.id,
        undefined,
      ),
    );
  });

  it("reveals the model picker and seeds the preset default once Test passes", async () => {
    settingsApiMock.testProvider.mockResolvedValue({ ok: true, model: "cli:claude" });
    settingsApiMock.updateProvider.mockResolvedValue({
      ...freshCliProvider,
      cli_model: "haiku",
    });
    renderCard(freshCliProvider, "haiku");

    fireEvent.click(
      screen.getByRole("button", {
        name: `Test connection for ${freshCliProvider.name}`,
      }),
    );

    await waitFor(() =>
      expect(
        screen.getByLabelText(`Model for ${freshCliProvider.name}`),
      ).toBeInTheDocument(),
    );
    expect(settingsApiMock.updateProvider).toHaveBeenCalledWith(
      freshCliProvider.id,
      {
        name: freshCliProvider.name,
        base_url: freshCliProvider.base_url,
        model: freshCliProvider.model,
        cli_model: "haiku",
      },
    );
  });

  it("a failed Test does not reveal the model picker", async () => {
    settingsApiMock.testProvider.mockResolvedValue({
      ok: false,
      error: "not logged in",
    });
    renderCard(freshCliProvider, "haiku");

    fireEvent.click(
      screen.getByRole("button", {
        name: `Test connection for ${freshCliProvider.name}`,
      }),
    );

    await waitFor(() => expect(settingsApiMock.testProvider).toHaveBeenCalled());
    expect(
      screen.queryByLabelText(`Model for ${freshCliProvider.name}`),
    ).not.toBeInTheDocument();
    expect(settingsApiMock.updateProvider).not.toHaveBeenCalled();
  });

  it("an already-configured provider shows the model picker immediately, no Test required", () => {
    const verified: ProviderView = { ...freshCliProvider, cli_model: "sonnet" };
    renderCard(verified);

    expect(
      screen.getByLabelText(`Model for ${verified.name}`),
    ).toBeInTheDocument();
    expect(screen.queryByText(/click test below/i)).not.toBeInTheDocument();
  });

  it("commits a typed --model override via PATCH, leaving name/base_url/model untouched", async () => {
    const verified: ProviderView = { ...freshCliProvider, cli_model: "sonnet" };
    settingsApiMock.updateProvider.mockResolvedValue({
      ...verified,
      cli_model: "opus",
    });
    renderCard(verified);

    const input = screen.getByLabelText(`Model for ${verified.name}`);
    fireEvent.change(input, { target: { value: "opus" } });
    fireEvent.blur(input);

    await waitFor(() =>
      expect(settingsApiMock.updateProvider).toHaveBeenCalledWith(
        verified.id,
        {
          name: verified.name,
          base_url: verified.base_url,
          model: verified.model,
          cli_model: "opus",
        },
      ),
    );
  });
});
