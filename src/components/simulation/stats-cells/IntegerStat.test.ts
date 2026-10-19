import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/vue";
import IntegerStat from "./IntegerStat.vue";

describe("IntegerStat", () => {
  it("shows the plain number under 1000", () => {
    render(IntegerStat, { props: { label: "Instructions", value: 42 } });
    expect(screen.getByText("42")).toBeTruthy();
  });

  it("groups thousands with a space", () => {
    render(IntegerStat, { props: { label: "Cycles", value: 1500 } });
    expect(screen.getByText("1 500")).toBeTruthy();
  });

  it("groups millions with a space every three digits", () => {
    render(IntegerStat, { props: { label: "Cycles", value: 2_300_000 } });
    expect(screen.getByText("2 300 000")).toBeTruthy();
  });

  it("groups billions with a space every three digits", () => {
    render(IntegerStat, { props: { label: "Cycles", value: 4_000_000_000 } });
    expect(screen.getByText("4 000 000 000")).toBeTruthy();
  });

  it("rounds to the nearest whole number", () => {
    render(IntegerStat, { props: { label: "Cycles", value: 999_999.6 } });
    expect(screen.getByText("1 000 000")).toBeTruthy();
  });
});
