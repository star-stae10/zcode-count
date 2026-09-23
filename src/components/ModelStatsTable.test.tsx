import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ModelStatsTable } from "./ModelStatsTable";
import { ModelStat } from "../lib/api";

function row(over: Partial<ModelStat> = {}): ModelStat {
  return {
    model_id: "claude-sonnet",
    request_count: 2,
    input_tokens: 1200,
    output_tokens: 340,
    total_cost_usd: "0.01",
    unpriced_count: 0,
    ...over,
  };
}

const summary = {
  input_tokens: 5000,
  output_tokens: 900,
  total_cost_usd: "0.1234",
  request_count: 7,
  unpriced_count: 0,
};

describe("ModelStatsTable", () => {
  it("renders all columns and row values", () => {
    const html = renderToStaticMarkup(<ModelStatsTable rows={[row()]} summary={summary} />);
    for (const c of ["模型", "请求数", "输入", "输出", "总成本"]) {
      expect(html).toContain(c);
    }
    expect(html).toContain("claude-sonnet");
    expect(html).toContain("2");
    expect(html).toContain("1.2K");
    expect(html).toContain("340");
    expect(html).toContain("$0.0100");
  });

  it("renders the all-models summary row with aggregated values", () => {
    const html = renderToStaticMarkup(<ModelStatsTable rows={[row()]} summary={summary} />);
    expect(html).toContain("所有模型");
    expect(html).toContain("5.0K");
    expect(html).toContain("900");
    expect(html).toContain("$0.1234");
    expect(html).toContain("7");
  });

  it("shows — for a row when all its requests are unpriced", () => {
    const html = renderToStaticMarkup(
      <ModelStatsTable rows={[row({ unpriced_count: 2 })]} summary={summary} />,
    );
    expect(html).toContain("—");
    expect(html).not.toContain("$0.0100");
    expect(html).toContain("$0.1234");
  });

  it("shows — for the summary row when all requests are unpriced", () => {
    const html = renderToStaticMarkup(
      <ModelStatsTable rows={[row()]} summary={{ ...summary, unpriced_count: 7 }} />,
    );
    expect(html).toContain("—");
    expect(html).not.toContain("$0.1234");
    expect(html).toContain("$0.0100");
  });

  it("omits the summary row when summary is null", () => {
    const html = renderToStaticMarkup(<ModelStatsTable rows={[row()]} summary={null} />);
    expect(html).not.toContain("所有模型");
  });

  it("sorts models by cost descending", () => {
    const html = renderToStaticMarkup(
      <ModelStatsTable
        rows={[
          row({ model_id: "cheap", total_cost_usd: "0.001" }),
          row({ model_id: "pricey", total_cost_usd: "9.99" }),
          row({ model_id: "mid", total_cost_usd: "0.5" }),
        ]}
        summary={summary}
      />,
    );
    expect(html.indexOf("pricey")).toBeLessThan(html.indexOf("mid"));
    expect(html.indexOf("mid")).toBeLessThan(html.indexOf("cheap"));
  });
});
