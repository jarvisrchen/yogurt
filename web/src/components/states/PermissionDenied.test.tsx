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
    // Seed an active query so refetchQueries has something to act on.
    qc.setQueryData(permissionsKey, {
      screen_recording: "denied",
      microphone: "granted",
      owner: "Ghostty",
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

  it("names the app that owns the grant", () => {
    const qc = new QueryClient();
    qc.setQueryData(permissionsKey, {
      screen_recording: "denied",
      microphone: "granted",
      owner: "Ghostty",
    });
    render(
      <QueryClientProvider client={qc}>
        <PermissionDenied />
      </QueryClientProvider>,
    );
    expect(screen.getByText("Ghostty")).toBeInTheDocument();
  });
});
