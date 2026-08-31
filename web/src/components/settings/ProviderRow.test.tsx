/**
 * ProviderRow - review-fix regression coverage.
 *
 * Covers the two bugs the review found in the MODEL field's PATCH flow
 * (typing a full model id used to fire a PATCH per keystroke via the old
 * `ComboBox` "pick on Enter with nothing highlighted" bug) and the
 * abandoned-draft-key bug (Cancel left the draft in `apiKeyDraft` state,
 * so a later Refresh would still probe with a key the user just backed
 * out of).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ProviderView } from "../../lib/api/settings";

const settingsApiMock = vi.hoisted(() => ({
  updateProvider: vi.fn(),
  listProviderModels: vi.fn(),
  testProvider: vi.fn(),
}));

vi.mock("../../lib/api/settings", () => ({
  settingsApi: settingsApiMock,
}));

import { ProviderRow } from "./ProviderRow";

const provider: ProviderView = {
  id: "01ROW",
  name: "My provider",
  base_url: "https://example.com/v1",
  model: "gpt-4o-mini",
  is_active: false,
  created_at: 0,
  api_key_masked: null,
  adapter: "http",
};

function renderRow(opts: { onKeyClosed?: () => void } = {}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <ProviderRow
        provider={provider}
        presetModels={["gpt-4o", "gpt-4o-mini"]}
        onKeyClosed={opts.onKeyClosed}
      />
    </QueryClientProvider>,
  );
}

describe("ProviderRow - MODEL field", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    settingsApiMock.updateProvider.mockResolvedValue({ ...provider });
    settingsApiMock.listProviderModels.mockResolvedValue(["gpt-4o"]);
  });

  it("does not PATCH while typing, and PATCHes once on Enter", async () => {
    renderRow();
    const input = screen.getByLabelText(`Model for ${provider.name}`);

    fireEvent.change(input, { target: { value: "g" } });
    fireEvent.change(input, { target: { value: "gp" } });
    fireEvent.change(input, { target: { value: "gpt-4o" } });
    expect(settingsApiMock.updateProvider).not.toHaveBeenCalled();

    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() =>
      expect(settingsApiMock.updateProvider).toHaveBeenCalledTimes(1),
    );
    expect(settingsApiMock.updateProvider).toHaveBeenCalledWith(
      provider.id,
      expect.objectContaining({ model: "gpt-4o" }),
    );
  });

  it("PATCHes once when a listbox option is clicked", async () => {
    renderRow();
    const input = screen.getByLabelText(`Model for ${provider.name}`);
    fireEvent.focus(input);
    fireEvent.mouseDown(screen.getByRole("option", { name: "gpt-4o" }));

    await waitFor(() =>
      expect(settingsApiMock.updateProvider).toHaveBeenCalledTimes(1),
    );
    expect(settingsApiMock.updateProvider).toHaveBeenCalledWith(
      provider.id,
      expect.objectContaining({ model: "gpt-4o" }),
    );
  });
});

describe("ProviderRow - API key Cancel clears the draft", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    settingsApiMock.listProviderModels.mockResolvedValue(["gpt-4o"]);
  });

  it("Refresh probes without the abandoned draft, and Cancel notifies onKeyClosed", async () => {
    const onKeyClosed = vi.fn();
    renderRow({ onKeyClosed });

    fireEvent.click(screen.getByRole("button", { name: "Add key" }));
    fireEvent.change(
      screen.getByLabelText(`API key for ${provider.name}`),
      { target: { value: "sk-draft-key" } },
    );

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onKeyClosed).toHaveBeenCalledTimes(1);

    fireEvent.click(
      screen.getByRole("button", {
        name: `Refresh model list for ${provider.name}`,
      }),
    );

    await waitFor(() =>
      expect(settingsApiMock.listProviderModels).toHaveBeenCalledWith(
        provider.id,
        undefined,
      ),
    );
  });
});

describe("ProviderRow - cli adapter (LLM-4)", () => {
  const cliProvider: ProviderView = {
    ...provider,
    id: "01CLI",
    name: "Claude Code (local CLI)",
    base_url: "",
    model: "claude",
    adapter: "cli",
  };

  function renderCliRow() {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={qc}>
        <ProviderRow provider={cliProvider} presetModels={[]} />
      </QueryClientProvider>,
    );
  }

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("has no BASE URL / MODEL fields, no key controls, and a reachable Test button", () => {
    renderCliRow();

    expect(screen.queryByText("BASE URL")).not.toBeInTheDocument();
    expect(screen.queryByText("MODEL")).not.toBeInTheDocument();
    expect(
      screen.queryByLabelText(`Model for ${cliProvider.name}`),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /add key|replace key/i }),
    ).not.toBeInTheDocument();

    // No key exists at all, so the normal "draft or stored key" gate must
    // not apply - Test is reachable immediately.
    const testButton = screen.getByRole("button", {
      name: `Test connection for ${cliProvider.name}`,
    });
    expect(testButton).toBeEnabled();
  });

  it("Test hits the provider endpoint with no key argument", async () => {
    settingsApiMock.testProvider.mockResolvedValue({ ok: true, model: "cli:claude" });
    renderCliRow();

    fireEvent.click(
      screen.getByRole("button", {
        name: `Test connection for ${cliProvider.name}`,
      }),
    );

    await waitFor(() =>
      expect(settingsApiMock.testProvider).toHaveBeenCalledWith(
        cliProvider.id,
        undefined,
      ),
    );
  });
});
