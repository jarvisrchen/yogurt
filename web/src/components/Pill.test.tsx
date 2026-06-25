import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Pill, RecordingBadge, ProviderChip } from "./Pill";

describe("Pill", () => {
  it("renders children", () => {
    render(<Pill>Local-only · on</Pill>);
    expect(screen.getByText(/local-only · on/i)).toBeInTheDocument();
  });

  it("defaults to a neutral tone (line border, paper bg)", () => {
    const { container } = render(<Pill>x</Pill>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/rounded-pill/);
    expect(el.className).toMatch(/border/);
  });

  it("matcha tone uses matcha-soft bg and matcha text", () => {
    const { container } = render(<Pill tone="matcha">Local-only · on</Pill>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-mtsoft/);
    expect(el.className).toMatch(/text-matcha/);
  });

  it("blue tone uses blueberry-soft bg and blueberry text", () => {
    const { container } = render(<Pill tone="blue">Active</Pill>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-blsoft/);
    expect(el.className).toMatch(/text-blue/);
  });

  it("straw tone uses strawberry-soft bg with ink body text + strawberry border (MD-06)", () => {
    // text-straw on bg-strsoft is ~2.9:1 (fails AA). The straw tone uses
    // text-ink for body copy; the strawberry color is reserved for the
    // border (and decorative icons).
    const { container } = render(<Pill tone="straw">Recording</Pill>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-strsoft/);
    expect(el.className).toMatch(/text-ink/);
    expect(el.className).toMatch(/border-straw/);
    expect(el.className).not.toMatch(/text-straw/);
  });

  it("has no role by default (decorative pills should not pollute AT tree)", () => {
    const { container } = render(<Pill>neutral</Pill>);
    const el = container.firstChild as HTMLElement;
    expect(el.getAttribute("role")).toBeNull();
    expect(el.getAttribute("aria-live")).toBeNull();
  });

  it("applies role='status' + aria-live='polite' when status='status' (MD-07)", () => {
    const { container } = render(<Pill status="status">Enhancing…</Pill>);
    const el = container.firstChild as HTMLElement;
    expect(el.getAttribute("role")).toBe("status");
    expect(el.getAttribute("aria-live")).toBe("polite");
  });

  it("applies role='alert' + aria-live='assertive' when status='alert' (MD-07)", () => {
    const { container } = render(
      <Pill status="alert">Permission denied</Pill>,
    );
    const el = container.firstChild as HTMLElement;
    expect(el.getAttribute("role")).toBe("alert");
    expect(el.getAttribute("aria-live")).toBe("assertive");
  });
});

describe("RecordingBadge", () => {
  it("renders a pulsing dot + the timer", () => {
    const { container } = render(<RecordingBadge elapsed="12:04" />);
    const dot = container.querySelector('[data-testid="recording-dot"]');
    expect(dot).not.toBeNull();
    expect(dot!.className).toMatch(/animate-recpulse/);
    expect(screen.getByText("12:04")).toBeInTheDocument();
  });

  it("renders the timer in mono font", () => {
    render(<RecordingBadge elapsed="00:42" />);
    const timer = screen.getByText("00:42");
    expect(timer.className).toMatch(/font-mono/);
  });

  it("is a polite live region announcing 'Recording, MM:SS' (MD-07)", () => {
    const { container } = render(<RecordingBadge elapsed="00:42" />);
    const badge = container.firstChild as HTMLElement;
    expect(badge.getAttribute("role")).toBe("status");
    expect(badge.getAttribute("aria-live")).toBe("polite");
    expect(badge.getAttribute("aria-label")).toBe("Recording, 00:42");
  });
});

describe("ProviderChip", () => {
  it("renders provider name", () => {
    render(<ProviderChip name="Ollama" />);
    expect(screen.getByText("Ollama")).toBeInTheDocument();
  });

  it("shows active state with blue tone", () => {
    const { container } = render(<ProviderChip name="Minimax" active />);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-blsoft/);
  });

  it("shows local state with matcha tone", () => {
    const { container } = render(<ProviderChip name="whisper.cpp" local />);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-mtsoft/);
  });
});
