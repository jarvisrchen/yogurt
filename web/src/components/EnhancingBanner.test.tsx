import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { EnhancingBanner } from "./EnhancingBanner";

afterEach(() => cleanup());

describe("EnhancingBanner", () => {
  it("renders the load-bearing copy and locale-formatted char count when visible", () => {
    render(<EnhancingBanner visible={true} chars={1234} />);

    const banner = screen.getByTestId("enhancing-banner");
    expect(banner).toBeInTheDocument();
    // The unicode ellipsis is significant.
    expect(banner).toHaveTextContent("Weaving your notes into the transcript…");

    const count = screen.getByTestId("enhancing-char-count");
    expect(count).toHaveTextContent("1,234 chars");
  });

  it("omits the char count when chars is undefined", () => {
    render(<EnhancingBanner visible={true} />);
    expect(screen.getByTestId("enhancing-banner")).toBeInTheDocument();
    expect(screen.queryByTestId("enhancing-char-count")).toBeNull();
  });

  it("renders nothing when visible is false", () => {
    const { container } = render(
      <EnhancingBanner visible={false} chars={9999} />,
    );
    expect(container.firstChild).toBeNull();
    expect(screen.queryByTestId("enhancing-banner")).toBeNull();
  });
});
