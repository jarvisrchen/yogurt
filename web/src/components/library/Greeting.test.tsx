import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, act } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter } from "react-router";

vi.mock("../../hooks/useGreeting", () => ({ useGreeting: () => ({ greeting: "Hello" }) }));
vi.mock("../../lib/api/cloud", async () => {
  const actual = await vi.importActual<typeof import("../../lib/api/cloud")>("../../lib/api/cloud");
  return {
    ...actual,
    useCloudStatus: () => ({
      data: {
        connected: true,
        not_pushed: 1186,
        push: { sent: 206, total: 1400, retry_in: null, announced: true, left_secs: null },
      },
    }),
  };
});

import { Greeting } from "./Greeting";
import { PushPill } from "../PushPill";
import { PushProgress } from "../settings/cloud/CloudSection";
import { applyCloudPushEvent, clearCloudPush, useLivePush } from "../../hooks/useCloudPush";

function Progress() {
  const push = useLivePush({ sent: 206, total: 1400, retry_in: null, announced: true, left_secs: null });
  return push && <PushProgress push={push} />;
}

afterEach(() => {
  cleanup();
  act(() => clearCloudPush());
});

describe("live push counts", () => {
  it("one progress frame moves the pill, the greeting, and the Settings bar together", () => {
    render(
      <MemoryRouter>
        <PushPill />
        <Greeting count={1400} />
        <Progress />
      </MemoryRouter>,
    );
    expect(screen.getAllByText("Pushing 206 of 1,400")).toHaveLength(2);
    act(() => applyCloudPushEvent({ type: "cloud_push_progress", sent: 340, total: 1400, retry_in: null, left_secs: null }));
    expect(screen.getByTestId("push-pill")).toHaveTextContent("340 of 1,400");
    expect(screen.getAllByText("Pushing 340 of 1,400")).toHaveLength(2);
    expect(screen.queryByText("Pushing 206 of 1,400")).not.toBeInTheDocument();
  });
});
