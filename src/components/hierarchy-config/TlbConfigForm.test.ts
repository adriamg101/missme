import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/vue";
import TlbConfigForm from "./TlbConfigForm.vue";

const tlb = {
  pageSize: 4096,
  pageTableLevels: 1,
  pageWalkStart: "RAM",
  entries: 64,
  associativity: 4,
  replacement: "LRU" as const,
};

const targets = [
  { id: "l2", label: "L2" },
  { id: "l3", label: "L3" },
  { id: "RAM", label: "RAM" },
];

describe("TlbConfigForm", () => {
  it("offers every non-L1 level plus RAM as page-walk-start options, defaulting to RAM", () => {
    render(TlbConfigForm, { props: { tlb, pageWalkTargets: targets } });
    const select = screen.getByLabelText("Page walk start") as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.textContent)).toEqual(["L2", "L3", "RAM"]);
    expect(select.value).toBe("RAM");
  });

  it("emits an update patch when page size changes via the stepper", async () => {
    const { emitted } = render(TlbConfigForm, { props: { tlb, pageWalkTargets: targets } });
    await fireEvent.click(screen.getByLabelText("Incrementar"));
    expect(emitted().update[0]).toEqual([{ pageSize: 8192 }]);
  });

  it("rejects a page-table-levels value below 1", async () => {
    const { emitted } = render(TlbConfigForm, { props: { tlb, pageWalkTargets: targets } });
    const input = screen.getByLabelText("Page table levels") as HTMLInputElement;
    await fireEvent.update(input, "0");
    await fireEvent.change(input);
    expect(emitted().update.at(-1)).toEqual([{ pageTableLevels: 1 }]);
  });
});
