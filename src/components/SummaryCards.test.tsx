import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { SummaryCards } from "./SummaryCards";
import { Summary } from "../lib/api";

const base: Summary = {
  input_tokens: 900, output_tokens: 100, reasoning_tokens: 0,
  cache_read_tokens: 0, cache_creation_tokens: 0,
  request_count: 2, session_count: 1,
  total_cost_usd: "0.01", unpriced_count: 0,
  unpriced_tokens: 0, unpriced_models: 0,
};

describe("SummaryCards", () => {
  it("renders cost cards without warning when everything is priced", () => {
    const html = renderToStaticMarkup(
      <SummaryCards summary={base} range="all" onShowUnpriced={() => {}} onOpenPricing={() => {}} />,
    );
    expect(html).toContain("总成本");
    expect(html).toContain("$0.01000");
    expect(html).not.toContain("部分模型未配置定价");
    expect(html).not.toContain("查看未定价模型清单");
  });

  it("shows the unpriced warning with counts, tokens and confidence when models are unpriced", () => {
    const s: Summary = {
      ...base, input_tokens: 1600,
      unpriced_count: 1, unpriced_tokens: 800, unpriced_models: 1,
    };
    const html = renderToStaticMarkup(
      <SummaryCards summary={s} range="all" onShowUnpriced={() => {}} onOpenPricing={() => {}} />,
    );
    expect(html).toContain("部分模型未配置定价，当前成本仅统计已定价模型，实际成本可能高于显示值");
    expect(html).toContain("未定价模型 1 个");
    expect(html).toContain("未定价 token 量");
    expect(html).toContain("成本可信度");
    expect(html).toContain("查看未定价模型清单");
    expect(html).toContain("配置定价");
    // 部分未定价时总成本应显示 ≥ 前缀
    expect(html).toContain("≥ $0.01000");
  });

  it("renders nothing without summary", () => {
    const html = renderToStaticMarkup(
      <SummaryCards summary={null} range="all" onShowUnpriced={() => {}} onOpenPricing={() => {}} />,
    );
    expect(html).toBe("");
  });
});
