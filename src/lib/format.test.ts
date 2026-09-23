import { describe, it, expect } from "vitest";
import { formatTokens, formatCost, rangeToWindow } from "./format";

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

describe("rangeToWindow", () => {
  it("today covers from local midnight", () => {
    const { since, until } = rangeToWindow("today");
    expect(until).toBeGreaterThan(since);
  });
});
