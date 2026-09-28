import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { RequestLogTable } from "./RequestLogTable";
import { RequestLogRow } from "../lib/api";

function row(over: Partial<RequestLogRow> = {}): RequestLogRow {
  return {
    request_id: "req-1",
    provider_id: "anthropic",
    model_id: "claude-sonnet",
    input_tokens: 120,
    output_tokens: 56,
    cache_read_tokens: 34,
    total_cost_usd: "0.01",
    priced: true,
    duration_ms: 1500,
    first_token_ms: 300,
    status: "completed",
    started_at: Date.UTC(2026, 8, 23, 12, 34),
    query_source: "main_turn",
    price_tier: null,
    ...over,
  };
}

describe("RequestLogTable", () => {
  it("renders all columns and row values including cache-read subline", () => {
    const html = renderToStaticMarkup(<RequestLogTable rows={[row()]} names={{}} />);
    for (const c of ["时间", "供应商", "计费模型", "输入", "输出", "总成本", "计费档", "用时/首字", "状态", "来源"]) {
      expect(html).toContain(c);
    }
    expect(html).toContain("anthropic");
    expect(html).toContain("claude-sonnet");
    expect(html).toContain("120");
    expect(html).toContain("R34");
    expect(html).toContain("56");
    expect(html).toContain("$0.01000");
    expect(html).toContain("1.5s / 0.3s");
    expect(html).toContain("main_turn");
    expect(html).toMatch(/\d\d\/\d\d \d\d:\d\d/);
  });

  it("shows the mapped provider name and keeps the raw id in title", () => {
    const html = renderToStaticMarkup(
      <RequestLogTable rows={[row({ provider_id: "new-provider" })]} names={{ "new-provider": "OpenCode Go" }} />,
    );
    expect(html).toContain("OpenCode Go");
    expect(html).toContain('title="new-provider"');
    // 名称映射生效时，原始 ID 不应再作为可见文本出现（仅存在于 title）
    expect(html).not.toContain(">new-provider<");
  });

  it("falls back to the raw provider_id without title when unmapped", () => {
    const html = renderToStaticMarkup(
      <RequestLogTable rows={[row({ provider_id: "new-provider" })]} names={{}} />,
    );
    expect(html).toContain("new-provider");
    expect(html).not.toContain("title=");
  });

  it("shows dash when query_source is null", () => {
    const html = renderToStaticMarkup(<RequestLogTable rows={[row({ query_source: null })]} names={{}} />);
    expect(html).toContain("—");
  });

  it("renders billing tier column: 峰 for peak, 谷 for off_peak, — for null", () => {
    // 判档在后端（pricing/tier.rs），前端只按 price_tier 渲染。
    const peak = renderToStaticMarkup(<RequestLogTable rows={[row({ price_tier: "peak" })]} names={{}} />);
    expect(peak).toContain(">峰</td>");
    const offPeak = renderToStaticMarkup(<RequestLogTable rows={[row({ price_tier: "off_peak" })]} names={{}} />);
    expect(offPeak).toContain(">谷</td>");
    const none = renderToStaticMarkup(<RequestLogTable rows={[row()]} names={{}} />);
    expect(none).toContain("计费档");
    expect(none).toContain(">—</td>");
  });

  it("shows dash for unpriced cost and when timings are null", () => {
    const html = renderToStaticMarkup(
      <RequestLogTable rows={[row({ priced: false, duration_ms: null, first_token_ms: null })]} names={{}} />,
    );
    expect(html).toContain("—");
    expect(html).not.toContain("s /");
  });

  it("colors status by value", () => {
    const done = renderToStaticMarkup(<RequestLogTable rows={[row({ status: "completed" })]} names={{}} />);
    const err = renderToStaticMarkup(<RequestLogTable rows={[row({ status: "error" })]} names={{}} />);
    const other = renderToStaticMarkup(<RequestLogTable rows={[row({ status: "pending" })]} names={{}} />);
    expect(done).toContain("text-green-600");
    expect(err).toContain("text-red-600");
    expect(other).toContain("text-gray-500");
  });

  it("renders empty state when no rows", () => {
    const html = renderToStaticMarkup(<RequestLogTable rows={[]} names={{}} />);
    expect(html).toContain("暂无数据");
  });
});
