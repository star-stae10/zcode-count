import { describe, it, expect } from "vitest";
import { formatTokens, formatCost, formatCostWithUnpriced, rangeToWindow, rangeLabel } from "./format";

describe("formatTokens", () => {
  it("formats thousands and millions", () => {
    expect(formatTokens(999)).toBe("999");
    expect(formatTokens(1500)).toBe("1.5K");
    expect(formatTokens(2_300_000)).toBe("2.3M");
  });
});

describe("formatCost", () => {
  it("shows 5 decimals and dash when unpriced", () => {
    expect(formatCost("0.0006648", true)).toBe("$0.00066");
    expect(formatCost("0", false)).toBe("—");
  });
});

describe("formatCostWithUnpriced", () => {
  it("plain $ when nothing unpriced", () => {
    expect(formatCostWithUnpriced("0.01", 0, 2)).toBe("$0.01000");
    expect(formatCostWithUnpriced("0", 0, 0)).toBe("$0.00000");
  });

  it("— when every request is unpriced", () => {
    expect(formatCostWithUnpriced("0.01", 2, 2)).toBe("—");
    expect(formatCostWithUnpriced("0", 3, 3)).toBe("—");
  });

  it("≥ $ when only part is unpriced", () => {
    expect(formatCostWithUnpriced("0.01", 1, 2)).toBe("≥ $0.01000");
  });
});

describe("rangeToWindow", () => {
  it("today covers from local midnight", () => {
    const { since, until } = rangeToWindow("today");
    expect(until).toBeGreaterThan(since);
  });

  it("custom range spans local midnight to the next-day midnight", () => {
    const { since, until } = rangeToWindow("custom", "2026-01-02", "2026-01-03");
    expect(since).toBe(new Date(2026, 0, 2).getTime());
    expect(until).toBe(new Date(2026, 0, 4).getTime());
  });

  it("custom range falls back to everything when dates missing", () => {
    const { since } = rangeToWindow("custom");
    expect(since).toBe(0);
  });
});

describe("rangeLabel", () => {
  it("labels every range", () => {
    expect(rangeLabel("all")).toBe("全部");
    expect(rangeLabel("today")).toBe("当天");
    expect(rangeLabel("7d")).toBe("7 天");
    expect(rangeLabel("30d")).toBe("30 天");
    expect(rangeLabel("custom")).toBe("自定义");
  });
});
