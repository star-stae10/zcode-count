import { describe, it, expect } from "vitest";
import { formatTokens, formatCost, formatCostWithUnpriced, costConfidence, rangeToWindow, rangeLabel, exportFileName } from "./format";

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

describe("costConfidence", () => {
  it("is high when everything is priced", () => {
    expect(costConfidence(0, 1000).level).toBe("high");
    expect(costConfidence(0, 1000).label).toBe("高");
    expect(costConfidence(0, 0).level).toBe("high");
  });

  it("is medium when unpriced ratio is below 20%", () => {
    const c = costConfidence(100, 1000);
    expect(c.level).toBe("medium");
    expect(c.label).toContain("10.0%");
  });

  it("is low when unpriced ratio reaches 20%", () => {
    const c = costConfidence(200, 1000);
    expect(c.level).toBe("low");
    expect(c.label).toContain("20.0%");
  });
});

describe("exportFileName", () => {
  it("encodes the time range into the file name", () => {
    expect(exportFileName("all", undefined, undefined, "csv")).toBe("zcode-count-all.csv");
    expect(exportFileName("7d", undefined, undefined, "json")).toBe("zcode-count-7d.json");
    expect(exportFileName("30d", undefined, undefined, "csv")).toBe("zcode-count-30d.csv");
    expect(exportFileName("custom", "2026-09-01", "2026-09-28", "csv")).toBe("zcode-count-2026-09-01_to_2026-09-28.csv");
    expect(exportFileName("custom", undefined, undefined, "json")).toBe("zcode-count-custom.json");
  });

  it("uses today's local date for the today range", () => {
    const d = new Date();
    const p = (x: number) => String(x).padStart(2, "0");
    const today = `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
    expect(exportFileName("today", undefined, undefined, "csv")).toBe(`zcode-count-${today}.csv`);
  });
});
