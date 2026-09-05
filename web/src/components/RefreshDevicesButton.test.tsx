import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RefreshDevicesButton } from "./RefreshDevicesButton";

describe("RefreshDevicesButton", () => {
  it("invalidates both device-list queries on click", () => {
    const qc = new QueryClient();
    const spy = vi.spyOn(qc, "invalidateQueries");
    render(
      <QueryClientProvider client={qc}>
        <RefreshDevicesButton />
      </QueryClientProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: /refresh device list/i }));
    expect(spy).toHaveBeenCalledWith({ queryKey: ["audio-devices"] });
    expect(spy).toHaveBeenCalledWith({ queryKey: ["audio-output-devices"] });
  });
});
