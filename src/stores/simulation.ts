import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface SimulationState {
  isActive: boolean;
  isRunning: boolean;
  speedMs: number;
  currentLine: number | null;
}

export const useSimulationStore = defineStore("simulation", {
  state: (): SimulationState => ({
    isActive: false,
    isRunning: false,
    speedMs: 100,
    currentLine: null,
  }),
  actions: {
    async start(base: string, codegen: unknown, hierarchy: unknown, breakpoints: number[]) {
      await invoke("start_simulation", { base, codegen, hierarchy, breakpoints });
      this.isActive = true;
      this.isRunning = true;
      this.currentLine = null;
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
    },
    async setSpeed(ms: number) {
      await invoke("set_speed", { ms });
      this.speedMs = ms;
    },
    async listenForEvents(): Promise<() => void> {
      return listen<Record<string, unknown>>("sim:event", (event) => {
        const kind = event.payload.kind;
        if (kind === "instruction_complete") {
          this.currentLine = event.payload.sourceLine as number;
        } else if (kind === "simulation_paused") {
          this.isRunning = false;
        }
      });
    },
  },
});
