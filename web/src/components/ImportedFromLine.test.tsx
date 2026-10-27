import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { ImportedFromLine } from "./ImportedFromLine";

const URL_ = "https://notes.granola.ai/d/example";

describe("ImportedFromLine", () => {
  it("omits Captured by when the owner is the user", () => {
    render(
      <ImportedFromLine
        source={{ owner: { name: "Sam Example", email: "s@example.com", is_user: true }, web_url: URL_ }}
      />,
    );
    const line = screen.getByTestId("imported-from");
    expect(line).toHaveTextContent("Imported from Granola · Open in Granola");
    expect(line).not.toHaveTextContent("Captured by");
  });

  it("names the owner when someone else captured it", () => {
    render(
      <ImportedFromLine
        source={{ owner: { name: "Alex Example", email: "a@example.com", is_user: false }, web_url: URL_ }}
      />,
    );
    expect(screen.getByTestId("imported-from")).toHaveTextContent(
      "Imported from Granola · Captured by Alex Example · Open in Granola",
    );
  });

  it("falls back to the owner email and opens the link in a new tab", () => {
    render(
      <ImportedFromLine
        source={{ owner: { name: null, email: "a@example.com", is_user: false }, web_url: URL_ }}
      />,
    );
    expect(screen.getByTestId("imported-from")).toHaveTextContent("Captured by a@example.com");
    const link = screen.getByRole("link", { name: "Open in Granola" });
    expect(link).toHaveAttribute("href", URL_);
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
  });

  it("drops a link that is not http(s)", () => {
    render(<ImportedFromLine source={{ web_url: "javascript:alert(1)" }} />);
    expect(screen.queryByRole("link")).toBeNull();
  });
});
