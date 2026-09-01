import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Button } from "./Button";

describe("Button", () => {
  it("renders children as label text", () => {
    render(<Button>New meeting</Button>);
    expect(screen.getByRole("button", { name: /new meeting/i })).toBeInTheDocument();
  });

  it("defaults to the primary variant", () => {
    render(<Button>Go</Button>);
    const btn = screen.getByRole("button");
    expect(btn.className).toMatch(/bg-blue/);
    expect(btn.className).toMatch(/text-white/);
  });

  it("renders secondary variant with border", () => {
    render(<Button variant="secondary">Cancel</Button>);
    const btn = screen.getByRole("button");
    expect(btn.className).toMatch(/bg-card/);
    expect(btn.className).toMatch(/border/);
    expect(btn.className).toMatch(/text-ink/);
  });

  it("renders ghost variant as transparent + muted", () => {
    render(<Button variant="ghost">Dismiss</Button>);
    const btn = screen.getByRole("button");
    expect(btn.className).toMatch(/bg-transparent/);
    expect(btn.className).toMatch(/text-mut/);
  });

  it("renders warn variant in strawberry", () => {
    render(<Button variant="warn">Mic muted</Button>);
    const btn = screen.getByRole("button");
    expect(btn.className).toMatch(/bg-straw/);
    expect(btn.className).toMatch(/text-white/);
  });

  it("fires onClick", () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Click</Button>);
    fireEvent.click(screen.getByRole("button"));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("respects disabled", () => {
    const onClick = vi.fn();
    render(
      <Button disabled onClick={onClick}>
        nope
      </Button>
    );
    fireEvent.click(screen.getByRole("button"));
    expect(onClick).not.toHaveBeenCalled();
    expect(screen.getByRole("button")).toBeDisabled();
  });

  it("supports type='submit'", () => {
    render(<Button type="submit">Submit</Button>);
    expect(screen.getByRole("button")).toHaveAttribute("type", "submit");
  });

  it("declares a visible focus-visible ring (WCAG 2.4.7 / MD-03)", () => {
    render(<Button>focusable</Button>);
    const btn = screen.getByRole("button");
    expect(btn.className).toMatch(/focus-visible:ring-2/);
    expect(btn.className).toMatch(/focus-visible:ring-blue\/40/);
    expect(btn.className).toMatch(/focus-visible:ring-offset-paper/);
    expect(btn.className).toMatch(/focus-visible:outline-none/);
  });

  it("appends caller className after variant classes (MD-02)", () => {
    render(<Button className="w-full">x</Button>);
    expect(screen.getByRole("button").className).toMatch(/w-full/);
  });
});
