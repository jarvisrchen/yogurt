import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { Logo } from "./Logo";

describe("Logo", () => {
  it("renders an SVG with the spoon-and-swirl mark", () => {
    const { container } = render(<Logo size={44} />);
    const svg = container.querySelector("svg");
    expect(svg).not.toBeNull();
    expect(svg!.getAttribute("viewBox")).toBe("0 0 44 44");
    expect(svg!.getAttribute("width")).toBe("44");
    expect(svg!.getAttribute("height")).toBe("44");
  });

  it("uses the brand colors (blueberry + strawberry)", () => {
    const { container } = render(<Logo />);
    const fills = Array.from(container.querySelectorAll("[fill]")).map((el) =>
      el.getAttribute("fill")
    );
    expect(fills).toContain("#5B4FC7"); // blueberry
    expect(fills).toContain("#E07A66"); // strawberry dot
  });

  it("defaults to 44px when no size is provided", () => {
    const { container } = render(<Logo />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("width")).toBe("44");
    expect(svg.getAttribute("height")).toBe("44");
  });

  it("forwards an aria-label when provided", () => {
    const { getByLabelText } = render(<Logo ariaLabel="Yogurt" />);
    expect(getByLabelText("Yogurt")).toBeInTheDocument();
  });

  it("is decorative (aria-hidden) by default — no ariaLabel", () => {
    const { container } = render(<Logo />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("aria-hidden")).toBe("true");
    expect(svg.getAttribute("role")).toBeNull();
    expect(svg.getAttribute("aria-label")).toBeNull();
  });

  it("uses role='img' when an ariaLabel is provided (drops aria-hidden)", () => {
    const { container } = render(<Logo ariaLabel="Yogurt" />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("role")).toBe("img");
    expect(svg.getAttribute("aria-label")).toBe("Yogurt");
    expect(svg.getAttribute("aria-hidden")).toBeNull();
  });

  it("sets focusable='false' so IE/legacy Edge don't stop tab order on the SVG", () => {
    const { container } = render(<Logo />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("focusable")).toBe("false");
  });
});
