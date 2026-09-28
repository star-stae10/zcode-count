import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ProviderStatsTable } from "./ProviderStatsTable";
import { ProviderStat } from "../lib/api";

function row(over: Partial<ProviderStat> = {}): ProviderStat {
  return {
    provider_id: "anthropic",
    request_count: 3,
    input_tokens: 1200,
    output_tokens: 340,
    total_cost_usd: "0.01",
    unpriced_count: 0,
    ...over,
  };
}

describe("ProviderStatsTable", () => {
  it("renders all columns and row values", () => {
    const html = renderToStaticMarkup(<ProviderStatsTable rows={[row()]} names={{}} />);
    for (const c of ["供应商", "请求数", "输入", "输出", "总成本"]) {
      expect(html).toContain(c);
    }
    expect(html).toContain("anthropic");
    expect(html).toContain("3");
    expect(html).toContain("1.2K");
    expect(html).toContain("340");
    expect(html).toContain("$0.01000");
  });

  it("renders multiple providers in given order", () => {
    const html = renderToStaticMarkup(
      <ProviderStatsTable rows={[row({ provider_id: "openai" }), row({ provider_id: "anthropic" })]} names={{}} />,
    );
    expect(html.indexOf("openai")).toBeLessThan(html.indexOf("anthropic"));
  });

  it("shows the mapped provider name and keeps the raw id in title", () => {
    const html = renderToStaticMarkup(
      <ProviderStatsTable rows={[row({ provider_id: "new-provider" })]} names={{ "new-provider": "OpenCode Go" }} />,
    );
    expect(html).toContain("OpenCode Go");
    expect(html).toContain('title="new-provider"');
    expect(html).not.toContain(">new-provider<");
  });

  it("falls back to the raw provider_id without title when unmapped", () => {
    const html = renderToStaticMarkup(
      <ProviderStatsTable rows={[row({ provider_id: "new-provider" })]} names={{}} />,
    );
    expect(html).toContain("new-provider");
    expect(html).not.toContain("title=");
  });

  it("shows — for cost when all requests are unpriced", () => {
    const html = renderToStaticMarkup(
      <ProviderStatsTable rows={[row({ unpriced_count: 3 })]} names={{}} />,
    );
    expect(html).toContain("—");
    expect(html).not.toContain("$0.01000");
  });

  it("shows ≥ cost when only some requests are priced", () => {
    const html = renderToStaticMarkup(
      <ProviderStatsTable rows={[row({ unpriced_count: 2 })]} names={{}} />,
    );
    expect(html).toContain("≥ $0.01000");
  });
});
