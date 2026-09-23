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
    ...over,
  };
}

describe("ProviderStatsTable", () => {
  it("renders all columns and row values", () => {
    const html = renderToStaticMarkup(<ProviderStatsTable rows={[row()]} />);
    for (const c of ["供应商", "请求数", "输入", "输出", "总成本"]) {
      expect(html).toContain(c);
    }
    expect(html).toContain("anthropic");
    expect(html).toContain("3");
    expect(html).toContain("1.2K");
    expect(html).toContain("340");
    expect(html).toContain("$0.0100");
  });

  it("renders multiple providers in given order", () => {
    const html = renderToStaticMarkup(
      <ProviderStatsTable rows={[row({ provider_id: "openai" }), row({ provider_id: "anthropic" })]} />,
    );
    expect(html.indexOf("openai")).toBeLessThan(html.indexOf("anthropic"));
  });
});
