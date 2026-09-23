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
    ...over,
  };
}

describe("RequestLogTable", () => {
  it("renders all columns and row values including cache-read subline", () => {
    const html = renderToStaticMarkup(<RequestLogTable rows={[row()]} />);
    for (const c of ["时间", "供应商", "计费模型", "输入", "输出", "总成本", "用时/首字", "状态", "来源"]) {
      expect(html).toContain(c);
    }
    expect(html).toContain("anthropic");
    expect(html).toContain("claude-sonnet");
    expect(html).toContain("120");
    expect(html).toContain("R34");
    expect(html).toContain("56");
    expect(html).toContain("$0.0100");
    expect(html).toContain("1.5s / 0.3s");
    expect(html).toContain("main_turn");
    expect(html).toMatch(/\d\d\/\d\d \d\d:\d\d/);
  });

  it("shows dash when query_source is null", () => {
    const html = renderToStaticMarkup(<RequestLogTable rows={[row({ query_source: null })]} />);
    expect(html).toContain("—");
  });

  it("shows dash for unpriced cost and when timings are null", () => {
    const html = renderToStaticMarkup(
      <RequestLogTable rows={[row({ priced: false, duration_ms: null, first_token_ms: null })]} />,
    );
    expect(html).toContain("—");
    expect(html).not.toContain("s /");
  });

  it("colors status by value", () => {
    const done = renderToStaticMarkup(<RequestLogTable rows={[row({ status: "completed" })]} />);
    const err = renderToStaticMarkup(<RequestLogTable rows={[row({ status: "error" })]} />);
    const other = renderToStaticMarkup(<RequestLogTable rows={[row({ status: "pending" })]} />);
    expect(done).toContain("text-green-600");
    expect(err).toContain("text-red-600");
    expect(other).toContain("text-gray-500");
  });

  it("renders empty state when no rows", () => {
    const html = renderToStaticMarkup(<RequestLogTable rows={[]} />);
    expect(html).toContain("暂无数据");
  });
});
