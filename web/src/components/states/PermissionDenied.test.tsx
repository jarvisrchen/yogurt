/**
 * PermissionDenied honesty-pass test (fast task).
 *
 * The old "Restart Yogurt" button just called `window.location.reload()` —
 * a lie, since reloading the browser tab restarts nothing on the backend.
 * Asserts the replacement "Check again" button instead triggers a
 * refetch of the shared permission query, and that the copy no longer
 * tells the user to "Toggle Yogurt on".
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { permissionsKey } from "../../lib/api/audio";
import { PermissionDenied } from "./PermissionDenied";

describe("PermissionDenied", () => {
  it("does not claim there's a 'Yogurt' toggle in System Settings", () => {
    const qc = new QueryClient();
    render(
      <QueryClientProvider client={qc}>
        <PermissionDenied />
      </QueryClientProvider>,
    );
    expect(screen.queryByText(/toggle yogurt on/i)).not.toBeInTheDocument();
    expect(screen.getByText(/terminal/i)).toBeInTheDocument();
  });

  it("'Check again' refetches the permission query instead of reloading the page", async () => {
    const qc = new QueryClient();
    // Seed an active query under `permissionsKey` so refetchQueries has
    // something to act on (mirrors how Library.tsx keeps the hook mounted
    // alongside this component).
    qc.setQueryData(permissionsKey, {
      screen_recording: "denied",
      microphone: "granted",
    });
    const refetchSpy = vi.spyOn(qc, "refetchQueries");

    render(
      <QueryClientProvider client={qc}>
        <PermissionDenied />
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
