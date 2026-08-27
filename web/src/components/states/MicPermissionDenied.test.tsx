/**
 * MicPermissionDenied honesty-pass test (fast task) — parallel to
 * PermissionDenied.test.tsx. Same bug, same fix: "Restart Yogurt" used to
 * just reload the page; "Check again" now refetches the shared
 * permission query.
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { permissionsKey } from "../../lib/api/audio";
import { MicPermissionDenied } from "./MicPermissionDenied";

describe("MicPermissionDenied", () => {
  it("does not claim there's a 'Yogurt' toggle in System Settings", () => {
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <MicPermissionDenied />
      </QueryClientProvider>,
    );
    expect(screen.queryByText(/toggle yogurt on/i)).not.toBeInTheDocument();
    expect(screen.getByText(/terminal/i)).toBeInTheDocument();
  });

  it("'Check again' refetches the permission query instead of reloading the page", async () => {
    const qc = new QueryClient();
    qc.setQueryData(permissionsKey, {
      screen_recording: "granted",
      microphone: "denied",
    });
    const refetchSpy = vi.spyOn(qc, "refetchQueries");

    render(
      <QueryClientProvider client={qc}>
        <MicPermissionDenied />
      </QueryClientProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: /check again/i }));

    await waitFor(() => {
      expect(refetchSpy).toHaveBeenCalledWith({
        queryKey: permissionsKey,
        type: "active",
      });
    });
  });
});
