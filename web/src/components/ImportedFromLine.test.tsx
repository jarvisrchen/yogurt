import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { ImportedFromLine } from "./ImportedFromLine";

describe("ImportedFromLine", () => {
  it("omits Captured by when the owner is the user", () => {
    render(
      <ImportedFromLine
        source={{ owner: { name: "Sam Example", email: "s@example.com", is_user: true } }}
      />,
    );
    expect(screen.getByTestId("imported-from")).toHaveTextContent(/^Imported from Granola$/);
  });

  it("names the owner when someone else captured it", () => {
    render(
      <ImportedFromLine
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
        source={{ owner: { name: null, email: "a@example.com", is_user: false } }}
      />,
    );
    expect(screen.getByTestId("imported-from")).toHaveTextContent("Captured by a@example.com");
  });

  it("shows no link back to Granola", () => {
    render(<ImportedFromLine source={{ web_url: "https://notes.granola.ai/d/example" }} />);
    expect(screen.queryByRole("link")).toBeNull();
  });
});
