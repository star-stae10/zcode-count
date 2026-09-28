import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { UnpricedDialog } from "./UnpricedDialog";
import { UnpricedModelRow } from "../lib/api";

const row: UnpricedModelRow = {
  provider_id: "p1", model_id: "m1",
  request_count: 3, total_tokens: 1500,
  first_started_at: new Date(2026, 8, 1, 10, 0).getTime(),
  last_started_at: new Date(2026, 8, 2, 11, 0).getTime(),
  est_cost_low_usd: "0.0003", est_cost_high_usd: "0.0009",
};

describe("UnpricedDialog", () => {
  it("shows the warning copy and an empty state when all models are priced", () => {
    const html = renderToStaticMarkup(
      <UnpricedDialog rows={[]} loading={false} error={null} onPriceModel={() => {}} onClose={() => {}} />,
    );
    expect(html).toContain("部分模型未配置定价，当前成本仅统计已定价模型，实际成本可能高于显示值");
    expect(html).toContain("所有模型均已定价");
  });

  it("lists provider, model, calls, tokens, period, estimate and pricing entry", () => {
    const html = renderToStaticMarkup(
      <UnpricedDialog rows={[row]} loading={false} error={null} onPriceModel={() => {}} onClose={() => {}} />,
    );
    expect(html).toContain("p1");
    expect(html).toContain("m1");
    expect(html).toContain("1.5K");
    expect(html).toContain("$0.00030 ~ $0.00090");
    expect(html).toContain("补充定价");
  });

  it("marks unestimable rows and surfaces errors and loading", () => {
    const html = renderToStaticMarkup(
      <UnpricedDialog rows={[{ ...row, est_cost_low_usd: null, est_cost_high_usd: null }]}
        loading={false} error={null} onPriceModel={() => {}} onClose={() => {}} />,
    );
    expect(html).toContain("无法估算");
    const err = renderToStaticMarkup(
      <UnpricedDialog rows={[]} loading={false} error="boom" onPriceModel={() => {}} onClose={() => {}} />,
    );
    expect(err).toContain("boom");
    const loading = renderToStaticMarkup(
      <UnpricedDialog rows={[]} loading={true} error={null} onPriceModel={() => {}} onClose={() => {}} />,
    );
    expect(loading).toContain("加载中");
  });
});
