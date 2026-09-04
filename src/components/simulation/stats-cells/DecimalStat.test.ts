import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/vue";
import DecimalStat from "./DecimalStat.vue";

describe("DecimalStat", () => {
  it("fixes the value to the requested number of decimals", () => {
    render(DecimalStat, { props: { label: "CPI", value: 1.23456, decimals: 2 } });
    expect(screen.getByText("1.23")).toBeTruthy();
  });

  it("pads with trailing zeros when the value has fewer decimals than requested", () => {
    render(DecimalStat, { props: { label: "AMAT", value: 4, decimals: 2 } });
    expect(screen.getByText("4.00")).toBeTruthy();
  });
});
