import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router";
import { ImportMeetingButton } from "./ImportMeetingButton";

const bearerFetch = vi.fn();
vi.mock("../../lib/session", () => ({
  bearerFetch: (...a: unknown[]) => bearerFetch(...a),
  bearerJson: vi.fn(),
}));

function renderButton() {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<ImportMeetingButton />} />
          <Route path="/meeting/:id/post" element={<p>opened meeting</p>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const pick = (text: string) => {
  const file = new File([text], "Standup.yogurt");
  fireEvent.change(screen.getByLabelText("Import a meeting file"), { target: { files: [file] } });
  return file;
};

describe("ImportMeetingButton", () => {
  beforeEach(() => bearerFetch.mockReset());

  it("posts the file and opens the imported meeting", async () => {
    bearerFetch.mockResolvedValue(new Response(JSON.stringify({ id: "m9" }), { status: 201 }));
    renderButton();
    const file = pick('{"yogurt_meeting":1}');
    expect(await screen.findByText("opened meeting")).toBeInTheDocument();
    expect(bearerFetch).toHaveBeenCalledWith("/api/meetings/import", { method: "POST", body: file });
  });

  it("shows the server's reason when the file is rejected", async () => {
    bearerFetch.mockResolvedValue(
      new Response(JSON.stringify({ error: "this is not a yogurt meeting file" }), { status: 400 }),
    );
    renderButton();
    pick("hello");
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Couldn't import the meeting: this is not a yogurt meeting file",
      ),
    );
  });
});
