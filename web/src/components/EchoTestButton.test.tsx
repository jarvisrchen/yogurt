import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { EchoTestButton } from "./EchoTestButton";
import { audioApi } from "../lib/api/settings";

vi.mock("../lib/api/settings", async () => {
  const actual =
    await vi.importActual<typeof import("../lib/api/settings")>(
      "../lib/api/settings",
    );
  return {
    ...actual,
    audioApi: {
      ...actual.audioApi,
      testEcho: vi.fn(),
    },
  };
});

function renderButton(device = "BlackHole 2ch") {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <EchoTestButton device={device} />
    </QueryClientProvider>,
  );
}

describe("EchoTestButton", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders a Test button", () => {
    renderButton();
    expect(screen.getByRole("button", { name: /test/i })).toBeInTheDocument();
  });

  it("posts the device on click and shows the ok verdict", async () => {
    vi.mocked(audioApi.testEcho).mockResolvedValue({
      ok: true,
      model: "BlackHole 2ch · 440 Hz for 0.7 s",
    });
    renderButton("BlackHole 2ch");

    fireEvent.click(screen.getByRole("button", { name: /test/i }));

    await waitFor(() =>
      expect(audioApi.testEcho).toHaveBeenCalledWith("BlackHole 2ch"),
    );
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "✓ Tone played",
      ),
    );
  });

  it("shows the error verdict on failure", async () => {
    vi.mocked(audioApi.testEcho).mockResolvedValue({
      ok: false,
      error: "output device unavailable: not found: Nope",
    });
    renderButton("Nope");

    fireEvent.click(screen.getByRole("button", { name: /test/i }));

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "✗ output device unavailable: not found: Nope",
      ),
    );
  });
});
