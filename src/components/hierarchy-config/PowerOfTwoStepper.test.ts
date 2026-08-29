import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/vue";
import PowerOfTwoStepper from "./PowerOfTwoStepper.vue";

describe("PowerOfTwoStepper", () => {
  it("doubles the value on the increment button", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 64 } });
    await fireEvent.click(screen.getByLabelText("Increment"));
    expect(emitted()["update:modelValue"][0]).toEqual([128]);
  });

  it("halves the value on the decrement button", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 64 } });
    await fireEvent.click(screen.getByLabelText("Decrement"));
    expect(emitted()["update:modelValue"][0]).toEqual([32]);
  });

  it("does not go below min", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 1, min: 1 } });
    await fireEvent.click(screen.getByLabelText("Decrement"));
    expect(emitted()["update:modelValue"]).toBeUndefined();
  });

  it("does not go above max", async () => {
    const { emitted } = render(PowerOfTwoStepper, { props: { modelValue: 128, max: 128 } });
    await fireEvent.click(screen.getByLabelText("Increment"));
    expect(emitted()["update:modelValue"]).toBeUndefined();
  });

  it("renders the input readonly so the value can only change via the buttons", () => {
    render(PowerOfTwoStepper, { props: { modelValue: 64 } });
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    expect(input.readOnly).toBe(true);
    expect(input.value).toBe("64");
  });
});
