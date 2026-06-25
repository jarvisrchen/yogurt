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
});
