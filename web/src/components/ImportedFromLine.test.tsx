import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { ImportedFromLine } from "./ImportedFromLine";

describe("ImportedFromLine", () => {
  it("omits Captured by when the owner is the user", () => {
    render(
      <ImportedFromLine
        name="granola"
        source={{ owner: { name: "Sam Example", email: "s@example.com", is_user: true } }}
      />,
    );
    expect(screen.getByTestId("imported-from")).toHaveTextContent(/^Imported from Granola$/);
  });

  it("names the owner when someone else captured it", () => {
    render(
      <ImportedFromLine
        name="granola"
        source={{ owner: { name: "Alex Example", email: "a@example.com", is_user: false } }}
      />,
    );
    expect(screen.getByTestId("imported-from")).toHaveTextContent(
      /^Imported from Granola · Captured by Alex Example$/,
    );
  });

  it("falls back to the owner email", () => {
    render(
      <ImportedFromLine
        name="granola"
        source={{ owner: { name: null, email: "a@example.com", is_user: false } }}
      />,
    );
    expect(screen.getByTestId("imported-from")).toHaveTextContent("Captured by a@example.com");
  });

  it("says a shared meeting came from a yogurt file", () => {
    render(<ImportedFromLine name="yogurt" source={{ imported_at: "2026-11-02T00:00:00Z" }} />);
    expect(screen.getByTestId("imported-from")).toHaveTextContent(/^Imported from a yogurt file$/);
  });

  it("shows no link back to Granola", () => {
    render(<ImportedFromLine name="granola" source={{ web_url: "https://notes.granola.ai/d/example" }} />);
    expect(screen.queryByRole("link")).toBeNull();
  });
});
