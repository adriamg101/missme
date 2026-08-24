import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { render, screen } from "@testing-library/vue";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
let capturedHandler: ((event: { payload: Record<string, unknown> }) => void) | null = null;
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((_channel: string, handler: typeof capturedHandler) => {
    capturedHandler = handler;
    return Promise.resolve(() => {});
  }),
}));

import RamView from "./RamView.vue";

beforeEach(() => {
  setActivePinia(createPinia());
  capturedHandler = null;
});

describe("RamView", () => {
  it("counts ram_access events", async () => {
    render(RamView);
    await Promise.resolve();
    capturedHandler!({ payload: { kind: "ram_access", outcome: "hit", sourceLine: 1 } });
    capturedHandler!({ payload: { kind: "ram_access", outcome: "hit", sourceLine: 2 } });
    await Promise.resolve();
    expect(screen.getByText("2", { selector: ".simulation-ram-accesses" })).toBeTruthy();
  });
});
