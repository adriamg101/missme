import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen } from "@testing-library/vue";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }) }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));

import App from "./App.vue";
import { useSimulationStore } from "./stores/simulation";

beforeEach(() => setActivePinia(createPinia()));

describe("App simulation panel swap", () => {
  it("shows the hierarchy config panel by default", () => {
    render(App);
    expect(document.querySelector(".global-config-bar")).toBeTruthy();
  });

  it("shows the simulation info panel while a simulation is active", () => {
    const store = useSimulationStore();
    store.isActive = true;
    render(App);
    expect(screen.getByLabelText("Stats")).toBeTruthy();
  });

  it("clicking the register box while active opens the registers view", async () => {
    const store = useSimulationStore();
    store.isActive = true;
    const { container } = render(App);
    const regBox = container.querySelector('[data-box-id="reg"]');
    expect(regBox).toBeTruthy();
    await (regBox as HTMLElement).dispatchEvent(new Event("click", { bubbles: true }));
    expect(store.activeView).toEqual({ kind: "registers" });
  });

  it("clicking the RAM box while active opens the ram view (VM disabled by default)", async () => {
    const store = useSimulationStore();
    store.isActive = true;
    const { container } = render(App);
    const ramBox = container.querySelector('[data-box-id="ram"]');
    expect(ramBox).toBeTruthy();
    await (ramBox as HTMLElement).dispatchEvent(new Event("click", { bubbles: true }));
    expect(store.activeView).toEqual({ kind: "ram" });
  });
});
