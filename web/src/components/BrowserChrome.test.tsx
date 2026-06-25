import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { BrowserChrome } from "./BrowserChrome";

describe("BrowserChrome", () => {
  it("renders the URL in the centered pill", () => {
    render(
      <BrowserChrome url="localhost:7878/welcome">
        <div>inner</div>
      </BrowserChrome>
    );
    expect(screen.getByText("localhost:7878/welcome")).toBeInTheDocument();
  });

  it("renders three traffic-light dots", () => {
    const { container } = render(
      <BrowserChrome url="x">
        <div />
      </BrowserChrome>
    );
    const dots = container.querySelectorAll('[data-testid="traffic-dot"]');
    expect(dots.length).toBe(3);
  });

  it("renders children inside the chrome body", () => {
    render(
      <BrowserChrome url="x">
        <p data-testid="content">hello</p>
      </BrowserChrome>
    );
    expect(screen.getByTestId("content")).toBeInTheDocument();
  });

  it("uses the window-shadow elevation", () => {
    const { container } = render(
      <BrowserChrome url="x">
        <div />
      </BrowserChrome>
    );
    const root = container.firstChild as HTMLElement;
    expect(root.className).toMatch(/shadow-/);
  });

  it("marks the chrome header aria-hidden so screenreaders skip the fake URL (MD-08)", () => {
    const { getByTestId } = render(
      <BrowserChrome url="localhost:7878/welcome">
        <div />
      </BrowserChrome>
    );
    const header = getByTestId("browser-chrome-header");
    expect(header.getAttribute("aria-hidden")).toBe("true");
  });
});
