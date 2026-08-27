/**
 * LocalSTTCard "Use Local" guard test (fast task).
 *
 * Local STT could previously be activated with a model that was never
 * downloaded — recording then failed at meeting start. The radio must be
 * disabled (and show a "Download the model first" hint) until the
 * currently-selected model's `downloaded` flag is true.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ModelView } from "../../lib/api/stt";

const modelsFixture: ModelView[] = [
  { name: "tiny.en", size_mb: 39, downloaded: true, intel_supported: true },
  { name: "small.en", size_mb: 487, downloaded: false, intel_supported: true },
];

vi.mock("../../lib/api/stt", () => ({
  useModels: vi.fn(() => ({
    data: modelsFixture,
    isLoading: false,
    isError: false,
    error: null,
  })),
  // LocalSTTCard always renders <ModelDownloadDialog>, which calls
  // useDownloadModel() unconditionally — stub it even though no test
  // here opens the dialog.
  useDownloadModel: vi.fn(() => ({ mutate: vi.fn(), isPending: false })),
}));

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

  it("does not block the radio when local is already the active provider", () => {
    // Guard only applies to the activation transition — once local is
    // active, the radio stays interactive even if a later model swap
    // temporarily selects an undownloaded model.
    renderCard({ active: true, selectedModel: "small.en" });

    const radio = screen.getByRole("radio", { name: /use local/i });
    expect(radio).not.toBeDisabled();
  });
});
