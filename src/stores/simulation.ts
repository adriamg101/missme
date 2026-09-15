import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type SimulationView =
  | { kind: "stats" }
  | { kind: "registers" }
  | { kind: "cache"; levelId: string; side: "unified" | "i" | "d" }
  | { kind: "ram" }
  | { kind: "tlb" }
  | { kind: "disk" }
  | { kind: "frames" }
  | { kind: "pageTable" };

interface SimulationState {
  isActive: boolean;
  isRunning: boolean;
  speedMs: number;
  currentLine: number | null;
  activeView: SimulationView;
  activeExtensions: string[];
}

export const useSimulationStore = defineStore("simulation", {
  state: (): SimulationState => ({
    isActive: false,
    isRunning: false,
    speedMs: 100,
    currentLine: null,
    activeView: { kind: "stats" },
    activeExtensions: [],
  }),
  actions: {
    async start(base: string, codegen: unknown, hierarchy: unknown, breakpoints: number[], extensions: string[] = []) {
      await invoke("start_simulation", { base, codegen, hierarchy, breakpoints });
      this.isActive = true;
      this.isRunning = true;
      this.currentLine = null;
      this.activeView = { kind: "stats" };
      this.activeExtensions = extensions;
    },
    async pause() {
      await invoke("pause_simulation", {});
      this.isRunning = false;
    },
    async resume() {
      await invoke("resume_simulation", {});
      this.isRunning = true;
    },
    async stepAction() {
      await invoke("step_action", {});
    },
    async stepInstruction() {
      await invoke("step_instruction", {});
    },
    async stop() {
      await invoke("stop_simulation", {});
      this.isActive = false;
      this.isRunning = false;
      this.currentLine = null;
      this.activeView = { kind: "stats" };
      this.activeExtensions = [];
    },
    async setSpeed(ms: number) {
      await invoke("set_speed", { ms });
      this.speedMs = ms;
    },
    openView(view: SimulationView) {
      this.activeView = view;
    },
    async listenForEvents(): Promise<() => void> {
      return listen<Record<string, unknown>>("sim:event", (event) => {
        const kind = event.payload.kind;
        if (kind === "instruction_complete") {
          this.currentLine = event.payload.sourceLine as number;
        } else if (kind === "simulation_paused") {
          this.isRunning = false;
          if (event.payload.sourceLine !== null && event.payload.sourceLine !== undefined) {
            this.currentLine = event.payload.sourceLine as number;
          }
        }
      });
    },
  },
});
