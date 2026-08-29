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
  it("offers every non-L1 level plus RAM as page-walk-start options, defaulting to RAM", async () => {
    render(TlbConfigForm, { props: { tlb, pageWalkTargets: targets } });
    await fireEvent.click(screen.getByLabelText("Page walk start"));
    const options = screen.getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["L2", "L3", "RAM"]);
    const selected = options.find((o) => o.getAttribute("aria-selected") === "true");
    expect(selected?.textContent).toBe("RAM");
  });

  it("choosing a page-walk-start option emits the update and closes the dropdown", async () => {
    const { emitted } = render(TlbConfigForm, { props: { tlb, pageWalkTargets: targets } });
    await fireEvent.click(screen.getByLabelText("Page walk start"));
    await fireEvent.click(screen.getByText("L2"));
    expect(emitted().update.at(-1)).toEqual([{ pageWalkStart: "l2" }]);
    expect(screen.queryAllByRole("option")).toHaveLength(0);
  });

  it("emits an update patch when page size changes via the stepper", async () => {
    const { emitted } = render(TlbConfigForm, { props: { tlb, pageWalkTargets: targets } });
    await fireEvent.click(screen.getAllByLabelText("Increment")[0]);
    expect(emitted().update[0]).toEqual([{ pageSize: 8192 }]);
  });

  it("rejects a page-table-levels value below 1", async () => {
    const { emitted } = render(TlbConfigForm, {
      props: { tlb: { ...tlb, pageTableLevels: 2 }, pageWalkTargets: targets },
    });
    const input = screen.getByLabelText("Page table levels") as HTMLInputElement;
    await fireEvent.update(input, "0");
    await fireEvent.blur(input);
    expect(emitted().update.at(-1)).toEqual([{ pageTableLevels: 1 }]);
  });
});
