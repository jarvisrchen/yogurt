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
import { fireEvent, render, screen } from "@testing-library/react";
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
});
