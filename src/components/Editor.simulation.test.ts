import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import Editor from "./Editor.vue";
import { useSimulationStore } from "../stores/simulation";

beforeEach(() => {
  setActivePinia(createPinia());
  invokeMock.mockReset();
});

describe("Editor simulation integration", () => {
  it("a successful compile starts a simulation with the codegen output", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "compile") {
        return Promise.resolve({
          error_count: 0,
          error_output: "",
          config: {},
          codegen: { layout: { text_base: 0x1000 }, entries: [] },
        });
      }
      return Promise.resolve(undefined);
    });

    render(Editor);
    await fireEvent.click(screen.getByLabelText("Compilar"));

    expect(invokeMock).toHaveBeenCalledWith(
      "start_simulation",
      expect.objectContaining({ codegen: { layout: { text_base: 0x1000 }, entries: [] } }),
    );
  });

  it("a failed compile does not start a simulation", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "compile") {
        return Promise.resolve({ error_count: 1, error_output: "1:1: error: bad", config: {}, codegen: null });
      }
      return Promise.resolve(undefined);
    });

    render(Editor);
    await fireEvent.click(screen.getByLabelText("Compilar"));

    expect(invokeMock).not.toHaveBeenCalledWith("start_simulation", expect.anything());
  });

  it("shows the simulation toolbar instead of language controls once active", async () => {
    const store = useSimulationStore();
    store.isActive = true;
    render(Editor);
    expect(screen.getByLabelText("Play/Pause")).toBeTruthy();
    expect(screen.queryByLabelText("Opciones de compilación")).toBeNull();
  });

  it("marks the current line in the gutter", async () => {
    const store = useSimulationStore();
    store.isActive = true;
    store.currentLine = 1;
    const { container } = render(Editor);
    expect(container.querySelector(".editor-current-line-arrow")).toBeTruthy();
  });
});
