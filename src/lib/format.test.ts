import { describe, it, expect } from "vitest";
import { formatTokens, formatCost, formatCostWithUnpriced, rangeToWindow } from "./format";

describe("formatTokens", () => {
  it("formats thousands and millions", () => {
    expect(formatTokens(999)).toBe("999");
    expect(formatTokens(1500)).toBe("1.5K");
    expect(formatTokens(2_300_000)).toBe("2.3M");
  });
});

describe("formatCost", () => {
  it("shows 4 decimals and dash when unpriced", () => {
    expect(formatCost("0.0006648", true)).toBe("$0.0007");
    expect(formatCost("0", false)).toBe("—");
  });
});

describe("formatCostWithUnpriced", () => {
  it("plain $ when nothing unpriced", () => {
    expect(formatCostWithUnpriced("0.01", 0, 2)).toBe("$0.0100");
    expect(formatCostWithUnpriced("0", 0, 0)).toBe("$0.0000");
  });

  it("— when every request is unpriced", () => {
    expect(formatCostWithUnpriced("0.01", 2, 2)).toBe("—");
    expect(formatCostWithUnpriced("0", 3, 3)).toBe("—");
  });

  it("≥ $ when only part is unpriced", () => {
    expect(formatCostWithUnpriced("0.01", 1, 2)).toBe("≥ $0.0100");
  });
});

describe("rangeToWindow", () => {
  it("today covers from local midnight", () => {
    const { since, until } = rangeToWindow("today");
    expect(until).toBeGreaterThan(since);
  });
});
