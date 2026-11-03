import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { AboutSection } from "./AboutSection";

function renderWithHealth(body: object, status = 200) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(body), { status })),
  );
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <AboutSection />
    </QueryClientProvider>,
  );
}

afterEach(() => vi.unstubAllGlobals());

describe("AboutSection", () => {
  it("shows the server version and links its release notes", async () => {
    renderWithHealth({ status: "ok", service: "yogurt-server", version: "2.2.0", mode: "release" });
    expect(await screen.findByTestId("about-version")).toHaveTextContent("v2.2.0");
    expect(screen.getByRole("link", { name: "Release notes" })).toHaveAttribute(
      "href",
      "https://github.com/jarvisrchen/yogurt/releases/tag/v2.2.0",
    );
    expect(screen.queryByText("dev build")).toBeNull();
  });

  it("marks a dev build and skips its release notes link", async () => {
    renderWithHealth({ status: "ok", service: "yogurt-server", version: "2.2.0", mode: "dev" });
    expect(await screen.findByText("dev build")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Release notes" })).toBeNull();
  });

  it("says Unavailable when the health check fails", async () => {
    renderWithHealth({}, 500);
    expect(await screen.findByText("Unavailable")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "GitHub" })).toHaveAttribute(
      "href",
      "https://github.com/jarvisrchen/yogurt",
    );
  });
});
