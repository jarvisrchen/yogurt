/**
 * LocalSTTCard "Use Local" guard test (fast task).
 *
 * Local STT could previously be activated with a model that was never
 * downloaded — recording then failed at meeting start. The radio must be
 * disabled (and show a "Download the model first" hint) until the
 * currently-selected model's `downloaded` flag is true.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ModelView } from "../../lib/api/stt";

const modelsFixture: ModelView[] = [
  { name: "tiny.en", size_mb: 39, downloaded: true, intel_supported: true },
  { name: "small.en", size_mb: 487, downloaded: false, intel_supported: true },
];

interface DeleteModelResult {
  mutate: (name: string) => void;
  isPending: boolean;
  isError: boolean;
  isSuccess: boolean;
  error: Error | null;
  data: { freed_bytes: number } | undefined;
  variables: string | undefined;
}

const deleteModelMock = vi.hoisted(() =>
  vi.fn(
    (): DeleteModelResult => ({
      mutate: vi.fn(),
      isPending: false,
      isError: false,
      isSuccess: false,
      error: null,
      data: undefined,
      variables: undefined,
    }),
  ),
);

vi.mock("../../lib/api/stt", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/api/stt")>();
  return {
    ...actual,
    useModels: vi.fn(() => ({
      data: modelsFixture,
      isLoading: false,
      isError: false,
      error: null,
    })),
    // LocalSTTCard always renders <ModelDownloadDialog>, which calls
    // useDownloadModel() unconditionally - stub it even though no test
    // here opens the dialog.
    useDownloadModel: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
    useDeleteModel: deleteModelMock,
  };
});

import { LocalSTTCard } from "./LocalSTTCard";

function renderCard(props: Partial<React.ComponentProps<typeof LocalSTTCard>> = {}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const onActivate = vi.fn();
  const onSelectModel = vi.fn();
  render(
    <QueryClientProvider client={qc}>
      <LocalSTTCard
        active={false}
        selectedModel="small.en"
        onSelectModel={onSelectModel}
        onActivate={onActivate}
        {...props}
      />
    </QueryClientProvider>,
  );
  return { onActivate, onSelectModel };
}

describe("LocalSTTCard — Use Local guard", () => {
  it("disables Use Local and shows a hint when the selected model isn't downloaded", () => {
    const { onActivate } = renderCard({ selectedModel: "small.en" });

    const radio = screen.getByRole("radio", { name: /use local/i });
    expect(radio).toBeDisabled();
    expect(screen.getByText(/download the model first/i)).toBeInTheDocument();

    fireEvent.click(radio);
    expect(onActivate).not.toHaveBeenCalled();
  });

  it("enables Use Local once the selected model is downloaded", () => {
    const { onActivate } = renderCard({ selectedModel: "tiny.en" });

    const radio = screen.getByRole("radio", { name: /use local/i });
    expect(radio).not.toBeDisabled();
    expect(screen.queryByText(/download the model first/i)).not.toBeInTheDocument();

    fireEvent.click(radio);
    expect(onActivate).toHaveBeenCalledTimes(1);
  });

  it("still disables the radio and shows the hint when local is already active but the model isn't downloaded", () => {
    // Regression for the guard gap: `activateBlocked` used to short-circuit
    // to `false` whenever `active` was already true, so a persisted-but-
    // broken combo (model deleted after activation, or a bad row from
    // before this guard existed) rendered as if nothing were wrong. The
    // guard must track disk truth regardless of which provider is active.
    renderCard({ active: true, selectedModel: "small.en" });

    const radio = screen.getByRole("radio", { name: /use local/i });
    expect(radio).toBeDisabled();
    expect(screen.getByText(/download the model first/i)).toBeInTheDocument();
  });

  it("keeps the radio enabled when local is active and the model is downloaded", () => {
    renderCard({ active: true, selectedModel: "tiny.en" });

    const radio = screen.getByRole("radio", { name: /use local/i });
    expect(radio).not.toBeDisabled();
    expect(screen.queryByText(/download the model first/i)).not.toBeInTheDocument();
  });
});

describe("LocalSTTCard - delete a downloaded model", () => {
  beforeEach(() => {
    deleteModelMock.mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
      isError: false,
      isSuccess: false,
      error: null,
      data: undefined,
      variables: undefined,
    });
  });

  it("confirming the trash for a downloaded, non-active model calls mutate(name)", () => {
    const mutate = vi.fn();
    deleteModelMock.mockReturnValue({
      mutate,
      isPending: false,
      isError: false,
      isSuccess: false,
      error: null,
      data: undefined,
      variables: undefined,
    });

    renderCard({ selectedModel: "small.en" });

    fireEvent.click(screen.getByRole("button", { name: /delete tiny\.en/i }));
    fireEvent.click(
      screen.getByRole("button", { name: /confirm delete tiny\.en/i }),
    );

    expect(mutate).toHaveBeenCalledWith("tiny.en");
    expect(mutate).toHaveBeenCalledTimes(1);
  });

  it("shows the freed-bytes line when the delete mutation succeeds", () => {
    deleteModelMock.mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
      isError: false,
      isSuccess: true,
      error: null,
      data: { freed_bytes: 3_094_000_000 },
      variables: "tiny.en",
    });

    renderCard();

    expect(screen.getByTestId("model-freed")).toHaveTextContent(
      "Deleted tiny.en - freed 3.0 GB",
    );
  });
});
