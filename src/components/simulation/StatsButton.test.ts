import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen, fireEvent } from "@testing-library/vue";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import StatsButton from "./StatsButton.vue";
import { useSimulationStore } from "../../stores/simulation";

beforeEach(() => setActivePinia(createPinia()));

describe("StatsButton", () => {
  it("clicking it opens the stats view", async () => {
    const store = useSimulationStore();
    store.openView({ kind: "registers" });
    render(StatsButton);
    await fireEvent.click(screen.getByLabelText("Stats"));
    expect(store.activeView).toEqual({ kind: "stats" });
  });
});
