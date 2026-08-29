import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen } from "@testing-library/vue";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

let capturedHandler: ((event: { payload: Record<string, unknown> }) => void) | null = null;
const listenMock = vi.fn((...args: unknown[]) => {
  capturedHandler = args[1] as typeof capturedHandler;
  return Promise.resolve(() => {});
});
vi.mock("@tauri-apps/api/event", () => ({ listen: (...args: unknown[]) => listenMock(...args) }));

import RegistersView from "./RegistersView.vue";
import { useSimulationStore } from "../../stores/simulation";
import { useHierarchyConfigStore } from "../../stores/hierarchyConfig";

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockReset();
  capturedHandler = null;
});

describe("RegistersView", () => {
  it("fetches and shows the initial register snapshot", async () => {
    invokeMock.mockResolvedValue({ int: Array(32).fill(0).map((_, i) => i), fp: Array(32).fill(0) });
    render(RegistersView);
    await Promise.resolve();
    await Promise.resolve();
    expect(invokeMock).toHaveBeenCalledWith("get_registers", {});
    expect(await screen.findByText("x5")).toBeTruthy();
  });

  it("shows the ABI alias name next to each integer register", async () => {
    invokeMock.mockResolvedValue({ int: Array(32).fill(0), fp: Array(32).fill(0) });
    render(RegistersView);
    await Promise.resolve();
    await Promise.resolve();
    expect(await screen.findByText("zero")).toBeTruthy();
    expect(screen.getByText("ra")).toBeTruthy();
    expect(screen.getByText("sp")).toBeTruthy();
  });

  it("hides the float register grid when the float extension is not active", async () => {
    invokeMock.mockResolvedValue({ int: Array(32).fill(0), fp: Array(32).fill(0) });
    const { container } = render(RegistersView);
    await Promise.resolve();
    await Promise.resolve();
    expect(container.querySelectorAll(".simulation-registers-grid").length).toBe(1);
    expect(screen.queryByText("fa0")).toBeNull();
  });

  it("shows the float register grid when the float extension is active", async () => {
    useSimulationStore().activeExtensions = ["F"];
    invokeMock.mockResolvedValue({ int: Array(32).fill(0), fp: Array(32).fill(0) });
    render(RegistersView);
    await Promise.resolve();
    await Promise.resolve();
    expect(await screen.findByText("fa0")).toBeTruthy();
  });

  it("updates a register's value live on a reg_write event", async () => {
    invokeMock.mockResolvedValue({ int: Array(32).fill(0), fp: Array(32).fill(0) });
    render(RegistersView);
    await Promise.resolve();
    await Promise.resolve();
    capturedHandler!({ payload: { kind: "reg_write", register: 3, value: 42, sourceLine: 1 } });
    await Promise.resolve();
    expect(await screen.findByText("0x0000002A")).toBeTruthy();
  });

  it("shows integer register values as zero-padded hex", async () => {
    invokeMock.mockResolvedValue({ int: Array(32).fill(0).map((_, i) => (i === 5 ? 255 : 0)), fp: Array(32).fill(0) });
    render(RegistersView);
    await Promise.resolve();
    await Promise.resolve();
    expect(await screen.findByText("0x000000FF")).toBeTruthy();
  });

  it("pads integer register values to 16 hex digits on RV64I", async () => {
    useSimulationStore().activeExtensions = [];
    useHierarchyConfigStore().isa = "RV64I";
    invokeMock.mockResolvedValue({ int: Array(32).fill(0).map((_, i) => (i === 5 ? 255 : 0)), fp: Array(32).fill(0) });
    render(RegistersView);
    await Promise.resolve();
    await Promise.resolve();
    expect(await screen.findByText("0x00000000000000FF")).toBeTruthy();
  });
});
