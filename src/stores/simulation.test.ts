import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const invokeMock = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

const listenMock = vi.fn();
vi.mock("@tauri-apps/api/event", () => ({ listen: (...args: unknown[]) => listenMock(...args) }));

import { useSimulationStore } from "./simulation";

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockClear();
  listenMock.mockClear();
});

describe("simulation store", () => {
  it("defaults to no active session", () => {
    const store = useSimulationStore();
    expect(store.isActive).toBe(false);
    expect(store.isRunning).toBe(false);
    expect(store.speedMs).toBe(100);
    expect(store.currentLine).toBeNull();
  });

  it("start invokes start_simulation and marks the session active and running", async () => {
    const store = useSimulationStore();
    await store.start("RV32I", { entries: [] }, { isa: "RV32I" }, [3]);
    expect(invokeMock).toHaveBeenCalledWith("start_simulation", {
      base: "RV32I",
      codegen: { entries: [] },
      hierarchy: { isa: "RV32I" },
      breakpoints: [3],
    });
    expect(store.isActive).toBe(true);
    expect(store.isRunning).toBe(true);
    expect(store.currentLine).toBeNull();
  });

  it("pause invokes pause_simulation and marks the session not running", async () => {
    const store = useSimulationStore();
    await store.start("RV32I", {}, {}, []);
    expect(store.isRunning).toBe(true);
    await store.pause();
    expect(invokeMock).toHaveBeenCalledWith("pause_simulation", {});
    expect(store.isRunning).toBe(false);
  });

  it("resume invokes resume_simulation and marks running", async () => {
    const store = useSimulationStore();
    await store.resume();
    expect(invokeMock).toHaveBeenCalledWith("resume_simulation", {});
    expect(store.isRunning).toBe(true);
  });

  it("stepAction invokes step_action", async () => {
    const store = useSimulationStore();
    await store.stepAction();
    expect(invokeMock).toHaveBeenCalledWith("step_action", {});
  });

  it("stepInstruction invokes step_instruction", async () => {
    const store = useSimulationStore();
    await store.stepInstruction();
    expect(invokeMock).toHaveBeenCalledWith("step_instruction", {});
  });

  it("stop invokes stop_simulation and resets state", async () => {
    const store = useSimulationStore();
    await store.start("RV32I", {}, {}, []);
    await store.stop();
    expect(invokeMock).toHaveBeenCalledWith("stop_simulation", {});
    expect(store.isActive).toBe(false);
    expect(store.isRunning).toBe(false);
    expect(store.currentLine).toBeNull();
  });

  it("setSpeed invokes set_speed and stores the value", async () => {
    const store = useSimulationStore();
    await store.setSpeed(250);
    expect(invokeMock).toHaveBeenCalledWith("set_speed", { ms: 250 });
    expect(store.speedMs).toBe(250);
  });

  it("listenForEvents updates currentLine on an instruction_complete event", async () => {
    let capturedHandler: ((event: { payload: Record<string, unknown> }) => void) | null = null;
    listenMock.mockImplementation((_channel: string, handler: typeof capturedHandler) => {
      capturedHandler = handler;
      return Promise.resolve(() => {});
    });

    const store = useSimulationStore();
    await store.listenForEvents();
    capturedHandler!({ payload: { kind: "instruction_complete", pc: 4096, sourceLine: 7 } });

    expect(store.currentLine).toBe(7);
  });

  it("listenForEvents marks the session not running on a simulation_paused event", async () => {
    let capturedHandler: ((event: { payload: Record<string, unknown> }) => void) | null = null;
    listenMock.mockImplementation((_channel: string, handler: typeof capturedHandler) => {
      capturedHandler = handler;
      return Promise.resolve(() => {});
    });

    const store = useSimulationStore();
    await store.start("RV32I", {}, {}, []);
    await store.listenForEvents();
    capturedHandler!({ payload: { kind: "simulation_paused", reason: "breakpoint" } });

    expect(store.isRunning).toBe(false);
  });
});
