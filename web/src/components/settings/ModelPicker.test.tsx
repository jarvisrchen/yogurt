/**
 * ModelPicker test suite (Phase 8 Plan 08-03).
 *
 * Verifies the two click-routing branches:
 *   - a downloaded pill click → `onSelect(name)` (and NOT `onRequestDownload`)
 *   - an undownloaded pill click → `onRequestDownload(name)` (and NOT `onSelect`)
 *
 * Visual / a11y assertions are out of scope for this suite — those are
 * the M1-Air bench's job.
 */
import { describe, it, expect, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { ModelPicker } from "./ModelPicker";
import type { ModelView } from "../../lib/api/stt";

const MODELS: ModelView[] = [
  {
    name: "tiny.en",
    size_mb: 39,
    downloaded: true,
    intel_supported: true,
  },
  {
    name: "small.en",
    size_mb: 487,
    downloaded: false,
    intel_supported: true,
  },
  {
    name: "medium.en",
    size_mb: 1500,
    downloaded: true,
    intel_supported: true,
  },
];

describe("ModelPicker", () => {
  it("calls onSelect when a downloaded model is clicked", () => {
    const onSelect = vi.fn();
    const onRequestDownload = vi.fn();

    render(
      <ModelPicker
        models={MODELS}
        selected="tiny.en"
        onSelect={onSelect}
        onRequestDownload={onRequestDownload}
      />,
    );

    const btn = screen.getByRole("button", { name: /select tiny\.en/i });
    fireEvent.click(btn);

    expect(onSelect).toHaveBeenCalledWith("tiny.en");
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onRequestDownload).not.toHaveBeenCalled();
  });

  it("calls onRequestDownload when an undownloaded model is clicked", () => {
    const onSelect = vi.fn();
    const onRequestDownload = vi.fn();

    render(
      <ModelPicker
        models={MODELS}
        selected="tiny.en"
        onSelect={onSelect}
        onRequestDownload={onRequestDownload}
      />,
    );

    const btn = screen.getByRole("button", {
      name: /download small\.en \(487 mb\)/i,
    });
    fireEvent.click(btn);

    expect(onRequestDownload).toHaveBeenCalledWith("small.en");
    expect(onRequestDownload).toHaveBeenCalledTimes(1);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("renders no trash button for undownloaded models or when onDelete is omitted", () => {
    const onSelect = vi.fn();
    const onRequestDownload = vi.fn();
    const onDelete = vi.fn();

    const { rerender } = render(
      <ModelPicker
        models={MODELS}
        selected="tiny.en"
        onSelect={onSelect}
        onRequestDownload={onRequestDownload}
        onDelete={onDelete}
      />,
    );
    expect(
      screen.queryByRole("button", { name: /delete small\.en/i }),
    ).not.toBeInTheDocument();

    rerender(
      <ModelPicker
        models={MODELS}
        selected="tiny.en"
        onSelect={onSelect}
        onRequestDownload={onRequestDownload}
      />,
    );
    expect(
      screen.queryByRole("button", { name: /delete tiny\.en/i }),
    ).not.toBeInTheDocument();
  });

  it("renders trash for a downloaded model but not for activeModelName", () => {
    render(
      <ModelPicker
        models={MODELS}
        selected="tiny.en"
        onSelect={vi.fn()}
        onRequestDownload={vi.fn()}
        onDelete={vi.fn()}
        activeModelName="tiny.en"
      />,
    );
    expect(
      screen.queryByRole("button", { name: /delete tiny\.en/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /delete medium\.en/i }),
    ).toBeInTheDocument();
  });

  it("confirms and calls onDelete without touching onSelect", () => {
    const onSelect = vi.fn();
    const onDelete = vi.fn();

    render(
      <ModelPicker
        models={MODELS}
        selected="tiny.en"
        onSelect={onSelect}
        onRequestDownload={vi.fn()}
        onDelete={onDelete}
        activeModelName="medium.en"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /delete tiny\.en/i }));
    const confirmBtn = screen.getByRole("button", {
      name: /confirm delete tiny\.en/i,
    });
    expect(confirmBtn).toHaveTextContent("Delete?");
    fireEvent.click(confirmBtn);

    expect(onDelete).toHaveBeenCalledWith("tiny.en");
    expect(onDelete).toHaveBeenCalledTimes(1);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("cancels the confirm without calling onDelete", () => {
    const onDelete = vi.fn();

    render(
      <ModelPicker
        models={MODELS}
        selected="tiny.en"
        onSelect={vi.fn()}
        onRequestDownload={vi.fn()}
        onDelete={onDelete}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /delete tiny\.en/i }));
    fireEvent.click(screen.getByRole("button", { name: /cancel/i }));

    expect(onDelete).not.toHaveBeenCalled();
    expect(
      screen.getByRole("button", { name: /delete tiny\.en/i }),
    ).toBeInTheDocument();
  });

  it("reverts the confirm on its own after 3s of inaction", () => {
    vi.useFakeTimers();
    try {
      render(
        <ModelPicker
          models={MODELS}
          selected="tiny.en"
          onSelect={vi.fn()}
          onRequestDownload={vi.fn()}
          onDelete={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByRole("button", { name: /delete tiny\.en/i }));
      expect(
        screen.getByRole("button", { name: /confirm delete tiny\.en/i }),
      ).toBeInTheDocument();

      act(() => {
        vi.advanceTimersByTime(3000);
      });

      expect(
        screen.queryByRole("button", { name: /confirm delete tiny\.en/i }),
      ).not.toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /delete tiny\.en/i }),
      ).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });
});
