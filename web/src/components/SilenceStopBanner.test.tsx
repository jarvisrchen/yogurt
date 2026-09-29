import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";

const keepSpy = vi.hoisted(() => vi.fn());
vi.mock("../lib/api/meetings", () => ({
  useKeepRecording: () => ({ mutate: keepSpy, isPending: false }),
}));

import { SilenceStopBanner } from "./SilenceStopBanner";

describe("SilenceStopBanner", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000_000);
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    keepSpy.mockClear();
  });

  it("counts down each second and stops at 0:00", () => {
    render(<SilenceStopBanner meetingId="m1" autoStopAt={1_000_000 + 62_000} />);
    expect(screen.getByText("1:02")).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(3_000);
    });
    expect(screen.getByText("0:59")).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(120_000);
    });
    expect(screen.getByText("0:00")).toBeInTheDocument();
  });

  it("Keep recording calls the endpoint for this meeting", () => {
    render(<SilenceStopBanner meetingId="m1" autoStopAt={1_000_000 + 30_000} />);
    fireEvent.click(screen.getByRole("button", { name: "Keep recording" }));
    expect(keepSpy).toHaveBeenCalledWith("m1");
  });

  describe("system notification", () => {
    afterEach(() => vi.unstubAllGlobals());

    function stubNotification(permission: NotificationPermission) {
      const created: Array<{ title: string; close: () => void }> = [];
      class Fake {
        static permission = permission;
        close = vi.fn();
        onclick: (() => void) | null = null;
        constructor(public title: string) {
          created.push(this);
        }
      }
      vi.stubGlobal("Notification", Fake);
      return created;
    }

    it("fires once per warning episode, not on later renders", () => {
      const created = stubNotification("granted");
      const { rerender, unmount } = render(
        <SilenceStopBanner meetingId="m1" autoStopAt={1_000_000 + 60_000} />,
      );
      expect(created).toHaveLength(1);
      expect(created[0].title).toBe("No audio for 4 minutes");

      rerender(
        <SilenceStopBanner meetingId="m1" autoStopAt={1_000_000 + 60_000} />,
      );
      act(() => {
        vi.advanceTimersByTime(5_000);
      });
      expect(created).toHaveLength(1);

      // Warning clears (banner unmounts) and recurs.
      unmount();
      render(<SilenceStopBanner meetingId="m1" autoStopAt={1_000_000 + 60_000} />);
      expect(created).toHaveLength(2);
    });

    it("does nothing without permission", () => {
      const created = stubNotification("default");
      render(<SilenceStopBanner meetingId="m1" autoStopAt={1_000_000 + 60_000} />);
      expect(created).toHaveLength(0);
    });
  });
});
