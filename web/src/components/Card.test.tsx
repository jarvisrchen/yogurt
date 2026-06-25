import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Card } from "./Card";

describe("Card", () => {
  it("renders children", () => {
    render(
      <Card>
        <h2>Hello</h2>
      </Card>
    );
    expect(screen.getByRole("heading", { name: /hello/i })).toBeInTheDocument();
  });

  it("applies card surface classes by default", () => {
    const { container } = render(<Card>x</Card>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/bg-card/);
    expect(el.className).toMatch(/rounded-card/);
    expect(el.className).toMatch(/shadow-card/);
  });

  it("supports an 'active' variant with a blueberry hairline border", () => {
    const { container } = render(<Card active>active</Card>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/border-blue/);
  });

  it("supports padding sizes", () => {
    const { container } = render(<Card padding="lg">x</Card>);
    const el = container.firstChild as HTMLElement;
    expect(el.className).toMatch(/p-8/);
  });

  it("renders as an <article> when as='article'", () => {
    const { container } = render(<Card as="article">x</Card>);
    expect(container.querySelector("article")).not.toBeNull();
  });
});
