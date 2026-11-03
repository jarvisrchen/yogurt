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

const pick = (...texts: string[]) => {
  const files = texts.map((t, i) => new File([t], `Standup ${i}.yogurt`));
  fireEvent.change(screen.getByLabelText("Import meeting files"), { target: { files } });
  return files[0];
};
const created = (id: string) => new Response(JSON.stringify({ id }), { status: 201 });
const rejected = (error: string) => new Response(JSON.stringify({ error }), { status: 400 });

describe("ImportMeetingButton", () => {
  beforeEach(() => bearerFetch.mockReset());

  it("posts the file and opens the imported meeting", async () => {
    bearerFetch.mockResolvedValue(created("m9"));
    renderButton();
    const file = pick('{"yogurt_meeting":1}');
    expect(await screen.findByText("opened meeting")).toBeInTheDocument();
    expect(bearerFetch).toHaveBeenCalledWith("/api/meetings/import", { method: "POST", body: file });
  });

  it("shows the server's reason when the file is rejected", async () => {
    bearerFetch.mockResolvedValue(rejected("this is not a yogurt meeting file"));
    renderButton();
    pick("hello");
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Couldn't import the meeting: this is not a yogurt meeting file",
      ),
    );
  });

  it("imports several files, stays on the library, and names the ones that failed", async () => {
    bearerFetch
      .mockResolvedValueOnce(created("m1"))
      .mockResolvedValueOnce(rejected("the meeting file is damaged"))
      .mockResolvedValueOnce(created("m3"));
    renderButton();
    pick("a", "b", "c");
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Couldn't import 1 of 3 files:");
    expect(alert).toHaveTextContent("Standup 1.yogurt: the meeting file is damaged");
    expect(bearerFetch).toHaveBeenCalledTimes(3);
    expect(screen.queryByText("opened meeting")).toBeNull();
  });

  it("says how many meetings arrived when several files all import", async () => {
    bearerFetch.mockResolvedValueOnce(created("m1")).mockResolvedValueOnce(created("m2"));
    renderButton();
    pick("a", "b");
    expect(await screen.findByRole("alert")).toHaveTextContent("Imported 2 meetings");
  });
});
