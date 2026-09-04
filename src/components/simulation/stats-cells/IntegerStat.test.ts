import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/vue";
import IntegerStat from "./IntegerStat.vue";

describe("IntegerStat", () => {
  it("shows the plain number under 1000", () => {
    render(IntegerStat, { props: { label: "Instructions", value: 42 } });
    expect(screen.getByText("42")).toBeTruthy();
  });

  it("formats thousands with a k suffix", () => {
    render(IntegerStat, { props: { label: "Cycles", value: 1500 } });
    expect(screen.getByText("1.5k")).toBeTruthy();
  });

  it("formats millions with an M suffix", () => {
    render(IntegerStat, { props: { label: "Cycles", value: 2_300_000 } });
    expect(screen.getByText("2.3M")).toBeTruthy();
  });

  it("formats billions with a G suffix", () => {
    render(IntegerStat, { props: { label: "Cycles", value: 4_000_000_000 } });
    expect(screen.getByText("4G")).toBeTruthy();
  });

  it("rolls over to the next unit instead of showing 1000 of the current one", () => {
    render(IntegerStat, { props: { label: "Cycles", value: 999_999 } });
    expect(screen.getByText("1M")).toBeTruthy();
  });
});
