import { describe, it, expect, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { EnhancingBanner } from "./EnhancingBanner";

afterEach(() => cleanup());

/**
 * EnhancingBanner contract per CONTEXT D-28 / PRD §5.11:
 *   - visible=true renders the load-bearing copy "Weaving your notes into
 *     the transcript…" (with the unicode ellipsis).
 *   - The character count formats with locale grouping (1234 → "1,234")
 *     and is only shown when `chars` is a non-negative number.
 *   - visible=false renders nothing (no banner, no empty wrapper, so the
 *     post-meeting layout doesn't reserve banner height when idle).
 */
describe("EnhancingBanner", () => {
  it("renders the load-bearing copy and locale-formatted char count when visible", () => {
    render(<EnhancingBanner visible={true} chars={1234} />);

    const banner = screen.getByTestId("enhancing-banner");
    expect(banner).toBeInTheDocument();
    // PRD §5.11 verbatim — the unicode ellipsis is significant.
    expect(banner).toHaveTextContent("Weaving your notes into the transcript…");

    const count = screen.getByTestId("enhancing-char-count");
    // en-US locale comma grouping (1234 → "1,234").
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
    // The component returns null, so the container has zero children.
    expect(container.firstChild).toBeNull();
    expect(screen.queryByTestId("enhancing-banner")).toBeNull();
  });
});
