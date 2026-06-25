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

  it("straw tone uses strawberry-soft bg and strawberry text", () => {
    const { container } = render(<Pill tone="straw">Recording</Pill>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-strsoft/);
    expect(el.className).toMatch(/text-straw/);
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
