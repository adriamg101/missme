import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/vue";
import TlbConfigForm from "./TlbConfigForm.vue";

const tlb = {
  pageWalkStart: "RAM",
  entries: 64,
  associativity: 4,
  replacement: "LRU" as const,
  latency: 1,
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

  it("emits an update patch when entries changes via the stepper", async () => {
    const { emitted } = render(TlbConfigForm, { props: { tlb, pageWalkTargets: targets } });
    await fireEvent.click(screen.getAllByLabelText("Increment")[0]);
    expect(emitted().update[0]).toEqual([{ entries: 128 }]);
  });

  it("shows the TLB latency field and emits an update patch when changed", async () => {
    const { getByLabelText, emitted } = render(TlbConfigForm, { props: { tlb, pageWalkTargets: targets } });
    const field = getByLabelText("TLB latency") as HTMLInputElement;
    expect(field.value).toBe("1");
    await fireEvent.update(field, "4");
    await fireEvent.blur(field);
    expect(emitted().update.at(-1)).toEqual([{ latency: 4 }]);
  });
});
