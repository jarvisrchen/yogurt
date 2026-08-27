/**
 * AddProviderForm test suite (fast task: wire the dead "+ Add" button).
 *
 * Covers required-field validation (submit disabled until name + base_url
 * + model are all non-empty) and that a valid submit posts via
 * `settingsApi.createProvider` and calls `onDone`.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const settingsApiMock = vi.hoisted(() => ({
  createProvider: vi.fn(),
}));

vi.mock("../../lib/api/settings", () => ({
  settingsApi: settingsApiMock,
}));

import { AddProviderForm } from "./AddProviderForm";

function renderForm(onDone = vi.fn()) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <AddProviderForm onDone={onDone} />
    </QueryClientProvider>,
  );
  return { onDone };
}

describe("AddProviderForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    settingsApiMock.createProvider.mockResolvedValue({
      id: "01NEWPROVIDER",
      name: "My provider",
      base_url: "https://example.com/v1",
      model: "gpt-4o-mini",
      is_active: false,
      created_at: 0,
      api_key_masked: null,
    });
  });

  it("disables submit until name, base URL, and model are all filled", () => {
    renderForm();
    const submit = screen.getByRole("button", { name: /add provider/i });
    expect(submit).toBeDisabled();

    fireEvent.change(screen.getByPlaceholderText("My provider"), {
      target: { value: "My provider" },
    });
    expect(submit).toBeDisabled();

    fireEvent.change(screen.getByPlaceholderText("https://…/v1"), {
      target: { value: "https://example.com/v1" },
    });
    expect(submit).toBeDisabled();

    fireEvent.change(screen.getByPlaceholderText("gpt-4o-mini"), {
      target: { value: "gpt-4o-mini" },
    });
    expect(submit).not.toBeDisabled();
  });

  it("posts via settingsApi.createProvider and calls onDone on success", async () => {
    const { onDone } = renderForm();

    fireEvent.change(screen.getByPlaceholderText("My provider"), {
      target: { value: "My provider" },
    });
    fireEvent.change(screen.getByPlaceholderText("https://…/v1"), {
      target: { value: "https://example.com/v1" },
    });
    fireEvent.change(screen.getByPlaceholderText("gpt-4o-mini"), {
      target: { value: "gpt-4o-mini" },
    });
    fireEvent.click(screen.getByRole("button", { name: /add provider/i }));

    await waitFor(() => {
      expect(settingsApiMock.createProvider).toHaveBeenCalledWith({
        name: "My provider",
        base_url: "https://example.com/v1",
        model: "gpt-4o-mini",
      });
    });
    await waitFor(() => expect(onDone).toHaveBeenCalledTimes(1));
  });

  it("calls onDone without posting when Cancel is clicked", () => {
    const { onDone } = renderForm();
    fireEvent.click(screen.getByRole("button", { name: /cancel/i }));
    expect(onDone).toHaveBeenCalledTimes(1);
    expect(settingsApiMock.createProvider).not.toHaveBeenCalled();
  });
});
