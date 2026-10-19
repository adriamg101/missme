// src/components/simulation/SimulationToolbar.test.ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";

const invokeMock = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import SimulationToolbar from "./SimulationToolbar.vue";
import { useSimulationStore } from "../../stores/simulation";

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockClear();
});

describe("SimulationToolbar", () => {
  it("clicking play/pause resumes when paused", async () => {
    render(SimulationToolbar);
    await fireEvent.click(screen.getByLabelText("Play/Pause"));
    expect(invokeMock).toHaveBeenCalledWith("resume_simulation", {});
  });

  it("clicking play/pause pauses when running", async () => {
    const store = useSimulationStore();
    store.isRunning = true;
    render(SimulationToolbar);
    await fireEvent.click(screen.getByLabelText("Play/Pause"));
    expect(invokeMock).toHaveBeenCalledWith("pause_simulation", {});
  });

  it("clicking stop invokes stop_simulation", async () => {
    render(SimulationToolbar);
    await fireEvent.click(screen.getByLabelText("Stop"));
    expect(invokeMock).toHaveBeenCalledWith("stop_simulation", {});
  });

  it("step buttons are disabled while running", () => {
    const store = useSimulationStore();
    store.isRunning = true;
    render(SimulationToolbar);
    expect(screen.getByLabelText("Step action")).toBeDisabled();
    expect(screen.getByLabelText("Step instruction")).toBeDisabled();
  });

  it("step action button invokes step_action when paused", async () => {
    render(SimulationToolbar);
    await fireEvent.click(screen.getByLabelText("Step action"));
    expect(invokeMock).toHaveBeenCalledWith("step_action", {});
  });

  it("shows the default speed (100ms) at the geometric midpoint of the log-scaled slider", () => {
    render(SimulationToolbar);
    // 10ms and 1000ms are the two ends of the scale; 100ms sits right in the
    // middle of that range, landing exactly at slider position 50 (out of 0 to 100).
    expect(screen.getByLabelText("Delay (ms)")).toHaveValue("50");
  });

  it("changing the speed slider maps its log-scaled position to ms before calling set_speed", async () => {
    render(SimulationToolbar);
    // Position 50 should map to 100ms, the middle of the range (see the test above).
    await fireEvent.update(screen.getByLabelText("Delay (ms)"), "50");
    expect(invokeMock).toHaveBeenCalledWith("set_speed", { ms: 100 });
  });

  it("the low end of the slider (position 0) maps to the maximum delay, the slowest speed", async () => {
    render(SimulationToolbar);
    await fireEvent.update(screen.getByLabelText("Delay (ms)"), "0");
    expect(invokeMock).toHaveBeenCalledWith("set_speed", { ms: 1000 });
  });

  it("the high end of the slider (position 100) maps to the minimum delay, the fastest speed", async () => {
    render(SimulationToolbar);
    await fireEvent.update(screen.getByLabelText("Delay (ms)"), "100");
    expect(invokeMock).toHaveBeenCalledWith("set_speed", { ms: 10 });
  });
});
