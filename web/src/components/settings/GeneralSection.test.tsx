import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { GeneralSection } from "./GeneralSection";

function mockStandalone(standalone: boolean) {
  vi.stubGlobal("matchMedia", (q: string) => ({
    matches: standalone && q.includes("standalone"),
    addEventListener: () => {},
  }));
}

/** Constructor spy carrying a static `.permission`, matching the real
 *  `Notification` API's shape closely enough for these tests. */
function mockNotification(permission: NotificationPermission) {
  class FakeNotification {
    static permission = permission;
    static requestPermission = vi.fn();
    close = vi.fn();
    constructor(
      public title: string,
      public options?: NotificationOptions,
    ) {}
  }
  vi.stubGlobal("Notification", FakeNotification);
  return FakeNotification;
}

function renderSection() {
  const qc = new QueryClient();
  return render(
    <QueryClientProvider client={qc}>
      <GeneralSection general={{
          port: 7878,
          open_browser_on_start: true,
          audio_input_device: "",
        audio_echo_feature: false,
        audio_echo_output_device: "",
        audio_echo_enabled: false,
        audio_echo_buffer: 512,
          first_run_completed: true,
          stt_provider: "local",
          stt_model: "",
          meeting_detection: true,
          meeting_detection_focus: true,
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

describe("GeneralSection notification permission (MTG-16)", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("offers to enable notifications when permission is default", async () => {
    const Fake = mockNotification("default");
    renderSection();
    const button = screen.getByRole("button", {
      name: "Enable system notifications",
    });
    Fake.requestPermission.mockResolvedValue("granted");
    fireEvent.click(button);
    await waitFor(() => expect(Fake.requestPermission).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByText("System notifications are on.")).toBeInTheDocument(),
    );
  });

  it("shows notifications are on when permission is granted", () => {
    mockNotification("granted");
    renderSection();
    expect(screen.getByText("System notifications are on.")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Enable system notifications" }),
    ).not.toBeInTheDocument();
  });

  it("explains how to unblock notifications when permission is denied", () => {
    mockNotification("denied");
    renderSection();
    expect(
      screen.getByText(
        "Notifications are blocked for this site in Chrome. Allow them in the site settings to get alerts.",
      ),
    ).toBeInTheDocument();
  });

  it("skips the control entirely when Notification is unsupported", () => {
    // No `mockNotification` call: jsdom does not define `Notification`.
    renderSection();
    expect(
      screen.queryByRole("button", { name: "Enable system notifications" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("System notifications are on.")).not.toBeInTheDocument();
  });
});

describe("GeneralSection standalone install hint (MTG-16)", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const hint = /Install yogurt as an app/;

  it("shows the hint when not running as an installed app", () => {
    mockStandalone(false);
    renderSection();
    expect(screen.getByText(hint)).toBeInTheDocument();
  });

  it("hides the hint when already running standalone", () => {
    mockStandalone(true);
    renderSection();
    expect(screen.queryByText(hint)).not.toBeInTheDocument();
  });
});
