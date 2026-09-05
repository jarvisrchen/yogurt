import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DeviceOptions } from "./DeviceOptions";

const devices = [
  { name: "MacBook Pro Microphone", is_default: true },
  { name: "BlackHole 2ch", is_default: false },
];

describe("DeviceOptions", () => {
  it("lists devices and marks the default", () => {
    render(<select><DeviceOptions devices={devices} selected="" /></select>);
    expect(screen.getAllByRole("option")).toHaveLength(2);
    expect(screen.getByRole("option", { name: "MacBook Pro Microphone (default)" })).toBeInTheDocument();
  });

  it("keeps an unplugged selection visible as unavailable", () => {
    render(<select><DeviceOptions devices={devices} selected="Porcelain PBP 2" /></select>);
    expect(screen.getByRole("option", { name: "Porcelain PBP 2 (unavailable)" })).toHaveValue("Porcelain PBP 2");
  });

  it("adds nothing when the selection is present", () => {
    render(<select><DeviceOptions devices={devices} selected="BlackHole 2ch" /></select>);
    expect(screen.getAllByRole("option")).toHaveLength(2);
  });
});
