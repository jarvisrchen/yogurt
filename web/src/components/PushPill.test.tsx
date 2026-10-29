import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, fireEvent, act } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { MemoryRouter } from "react-router";

const navigateSpy = vi.hoisted(() => vi.fn());
vi.mock("react-router", async () => {
  const actual = await vi.importActual<typeof import("react-router")>("react-router");
  return { ...actual, useNavigate: () => navigateSpy };
});

import { PushPill } from "./PushPill";
import {
  applyCloudPushEvent,
  applyCloudPushStatus,
  clearCloudPush,
  DONE_PILL_MS,
  type CloudPushEvent,
} from "../hooks/useCloudPush";

const send = (ev: CloudPushEvent) => act(() => applyCloudPushEvent(ev));
const progress = (sent: number, retry_in: number | null = null): CloudPushEvent => ({
  type: "cloud_push_progress",
  sent,
  total: 1412,
  retry_in,
  left_secs: null,
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  act(() => clearCloudPush());
  navigateSpy.mockClear();
});

const renderPill = () =>
  render(
    <MemoryRouter>
      <PushPill />
    </MemoryRouter>,
  );

describe("PushPill", () => {
  it("renders nothing until a push is announced", () => {
    renderPill();
    expect(screen.queryByTestId("push-pill")).not.toBeInTheDocument();
  });

  it("shows the count with thousands separators and opens Settings, Cloud", () => {
    renderPill();
    send(progress(340));
    const pill = screen.getByTestId("push-pill");
    expect(pill).toHaveTextContent("Pushing to Cloud · 340 of 1,412");
    fireEvent.click(pill);
    expect(navigateSpy).toHaveBeenCalledWith("/settings/cloud");
    expect(screen.getByTestId("push-pill")).toBeInTheDocument();
  });

  it("counts down a retry and returns to the count when the next frame lands", () => {
    vi.useFakeTimers();
    renderPill();
    send(progress(340, 45));
    expect(screen.getByTestId("push-pill")).toHaveTextContent("Retrying in 45s");
    act(() => vi.advanceTimersByTime(3000));
    expect(screen.getByTestId("push-pill")).toHaveTextContent("Retrying in 42s");
    send(progress(341));
    expect(screen.getByTestId("push-pill")).toHaveTextContent("Pushing to Cloud · 341 of 1,412");
  });

  it("goes away on its own a few seconds after the push finishes", () => {
    vi.useFakeTimers();
    renderPill();
    send({ type: "cloud_push_complete", pushed: 1412, deleted: 0 });
    act(() => vi.advanceTimersByTime(DONE_PILL_MS - 1));
    expect(screen.getByTestId("push-pill")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.queryByTestId("push-pill")).not.toBeInTheDocument();
  });

  it("closes a finished push from its close button, and keeps a failure up until then", () => {
    vi.useFakeTimers();
    renderPill();
    send({ type: "cloud_push_complete", pushed: 3, deleted: 0 });
    fireEvent.click(screen.getByRole("button", { name: "Close the Cloud push notice" }));
    expect(screen.queryByTestId("push-pill")).not.toBeInTheDocument();
    send({ type: "cloud_push_error", error: "Cloud unreachable", code: "network" });
    act(() => vi.advanceTimersByTime(60_000));
    fireEvent.click(screen.getByRole("button", { name: "Close the Cloud push notice" }));
    expect(screen.queryByTestId("push-pill")).not.toBeInTheDocument();
    expect(navigateSpy).not.toHaveBeenCalled();
  });

  it("reads Pushed on completion and opens Settings, Cloud when clicked", () => {
    renderPill();
    send({ type: "cloud_push_complete", pushed: 1412, deleted: 0 });
    const pill = screen.getByTestId("push-pill");
    expect(pill).toHaveTextContent("Pushed 1,412");
    fireEvent.click(pill);
    expect(navigateSpy).toHaveBeenCalledWith("/settings/cloud");
    expect(screen.queryByTestId("push-pill")).not.toBeInTheDocument();
  });

  it("shows a failure in the error tone", () => {
    renderPill();
    send(progress(10));
    send({ type: "cloud_push_error", error: "Cloud unreachable", code: "network" });
    const pill = screen.getByRole("button", { name: /show cloud push error/i });
    expect(pill).toHaveTextContent("Push failed");
    expect(pill.parentElement).toHaveClass("border-straw");
    fireEvent.click(pill);
    expect(navigateSpy).toHaveBeenCalledWith("/settings/cloud");
    expect(screen.queryByTestId("push-pill")).not.toBeInTheDocument();
  });

  it("disappears when the push is cancelled", () => {
    renderPill();
    send(progress(10));
    send({ type: "cloud_push_cancelled" });
    expect(screen.queryByTestId("push-pill")).not.toBeInTheDocument();
  });

  it("picks up a push already running from the server status, and drops it when it has ended", () => {
    renderPill();
    act(() => applyCloudPushStatus({ sent: 700, total: 1412, retry_in: null, announced: true, left_secs: null }));
    expect(screen.getByTestId("push-pill")).toHaveTextContent("700 of 1,412");
    act(() => applyCloudPushStatus(null));
    expect(screen.queryByTestId("push-pill")).not.toBeInTheDocument();
  });

  it("ignores a push the server never announced, and a stale status after the end", () => {
    renderPill();
    act(() => applyCloudPushStatus({ sent: 1, total: 3, retry_in: null, announced: false, left_secs: null }));
    expect(screen.queryByTestId("push-pill")).not.toBeInTheDocument();
    send({ type: "cloud_push_complete", pushed: 30, deleted: 0 });
    act(() => applyCloudPushStatus({ sent: 10, total: 30, retry_in: null, announced: true, left_secs: null }));
    expect(screen.getByTestId("push-pill")).toHaveTextContent("Pushed 30");
  });
});
