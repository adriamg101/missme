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

  it("changing the speed slider invokes set_speed", async () => {
    render(SimulationToolbar);
    await fireEvent.update(screen.getByLabelText("Delay (ms)"), "250");
    expect(invokeMock).toHaveBeenCalledWith("set_speed", { ms: 250 });
  });
});
