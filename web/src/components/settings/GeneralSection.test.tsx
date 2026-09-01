import { afterEach, describe, expect, it } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { GeneralSection } from "./GeneralSection";

function renderSection() {
  const qc = new QueryClient();
  return render(
    <QueryClientProvider client={qc}>
      <GeneralSection general={{
          port: 7878,
          open_browser_on_start: true,
          audio_input_device: "",
          first_run_completed: true,
          stt_provider: "local",
          stt_model: "",
        }} />
    </QueryClientProvider>,
  );
}

describe("GeneralSection appearance (UI-6)", () => {
  afterEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
  });

  it("switches to dark and persists, then back to system", () => {
    renderSection();
    const dark = screen.getByRole("radio", { name: "Dark" });
    fireEvent.click(dark);
    expect(dark).toHaveAttribute("aria-checked", "true");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("yogurt-theme")).toBe("dark");

    fireEvent.click(screen.getByRole("radio", { name: "System" }));
    expect(localStorage.getItem("yogurt-theme")).toBeNull();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("reads the stored preference on mount", () => {
    localStorage.setItem("yogurt-theme", "dark");
    renderSection();
    expect(screen.getByRole("radio", { name: "Dark" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });
});
