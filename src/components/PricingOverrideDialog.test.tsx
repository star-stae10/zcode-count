import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MatchedCount, PeakBadge, PricingOverrideDialog, SaveNotice, prefillPeak, validatePrices } from "./PricingOverrideDialog";
import { PriceOverride } from "../lib/api";

describe("PricingOverrideDialog", () => {
  it("renders provider options, four price inputs and save button", () => {
    const html = renderToStaticMarkup(
      <PricingOverrideDialog providers={["anthropic", "openai"]} names={{}} onClose={() => {}} onChanged={() => {}} />,
    );
    expect(html).toContain("定价覆盖");
    expect(html).toContain("供应商");
    expect(html).toContain("模型 ID");
    for (const label of ["输入单价", "输出单价", "缓存读取单价", "缓存创建单价"]) {
      expect(html).toContain(label);
    }
    // 供应商选项来自 providers，且带「自定义」
    expect(html).toContain("anthropic");
    expect(html).toContain("openai");
    expect(html).toContain("自定义");
    expect(html).toContain("保存");
    // number 输入受 min=0 约束
    expect(html).toContain('min="0"');
    // 无覆盖时的空态（SSR 不跑 effect，rows 初始为空）
    expect(html).toContain("暂无覆盖");
  });

  it("option value 仍为原始 provider_id，label 显示映射名称", () => {
    const html = renderToStaticMarkup(
      <PricingOverrideDialog
        providers={["opencode-go-chat", "new-provider"]}
        names={{ "opencode-go-chat": "OpenCode Go" }}
        onClose={() => {}}
        onChanged={() => {}}
      />,
    );
    // 有映射：value = 原始 ID，label = 名称，title 保留原始 ID
    expect(html).toContain('value="opencode-go-chat"');
    expect(html).toContain('title="opencode-go-chat"');
    expect(html).toContain(">OpenCode Go</option>");
    // 无映射：label 回退原始 ID，不加 title
    expect(html).toContain('<option value="new-provider">new-provider</option>');
  });

  it("覆盖列表表头含「命中记录数」列", () => {
    const html = renderToStaticMarkup(
      <PricingOverrideDialog providers={["p1"]} names={{}} onClose={() => {}} onChanged={() => {}} />,
    );
    expect(html).toContain("命中记录数");
  });
});

describe("峰谷定价（DeepSeek 峰谷适配）", () => {
  const peakRow: PriceOverride = {
    provider_id: "deepseek", model_id: "deepseek-v4.1-flash",
    input: "0.15", output: "0.6", cache_read: "0.003", cache_creation: "0.15",
    peak_input: "0.3", peak_output: "1.2", peak_cache_read: "0.006", peak_cache_creation: "0.3",
    matched_count: 5,
  };

  it("默认含「适配 DeepSeek 峰谷定价」按钮与「空闲时段单价」标签，不展开高峰时段单价输入", () => {
    const html = renderToStaticMarkup(
      <PricingOverrideDialog providers={["deepseek"]} names={{}} onClose={() => {}} onChanged={() => {}} />,
    );
    expect(html).toContain("适配 DeepSeek 峰谷定价");
    expect(html).toContain("空闲时段单价");
    expect(html).not.toContain("高峰时段单价");
    // 默认未启用峰谷，按钮文案不应是「关闭峰谷定价」
    expect(html).not.toContain("关闭峰谷定价");
  });

  it("PeakBadge：高峰四价齐全的覆盖显示「峰谷」徽标，半填或全空不显示", () => {
    expect(renderToStaticMarkup(<PeakBadge row={peakRow} />)).toContain("峰谷");
    const half = { ...peakRow, peak_output: null };
    expect(renderToStaticMarkup(<PeakBadge row={half} />)).not.toContain("峰谷");
    const none: PriceOverride = {
      ...peakRow,
      peak_input: null, peak_output: null, peak_cache_read: null, peak_cache_creation: null,
    };
    expect(renderToStaticMarkup(<PeakBadge row={none} />)).not.toContain("峰谷");
  });

  it("prefillPeak：空闲四价均已填且有效时按「高峰 = 空闲 ×2」预填，否则不预填", () => {
    expect(prefillPeak("0.15", "0.6", "0.003", "1")).toEqual(["0.3", "1.2", "0.006", "2"]);
    // 空闲组任一未填 → 不预填
    expect(prefillPeak("", "0.6", "0.003", "1")).toBeNull();
    expect(prefillPeak("0.15", "0.6", "0.003", "")).toBeNull();
    // 非数字 → 不预填
    expect(prefillPeak("abc", "0.6", "0.003", "1")).toBeNull();
  });
});

describe("SaveNotice", () => {
  it("repriced = 0 时显示琥珀色警示（覆盖未命中任何记录）", () => {
    const html = renderToStaticMarkup(<SaveNotice repriced={0} />);
    expect(html).toContain("已保存，但当前没有匹配的记录");
    expect(html).toContain("请检查模型 ID 是否与用量记录一致");
    expect(html).toContain("text-amber-600");
  });

  it("repriced > 0 时显示成功提示与重算条数", () => {
    const html = renderToStaticMarkup(<SaveNotice repriced={42} />);
    expect(html).toContain("已保存，已重算 42 条记录");
    expect(html).toContain("text-green-600");
    expect(html).not.toContain("text-amber-600");
  });
});

describe("MatchedCount", () => {
  it("0 命中时红字显示", () => {
    const html = renderToStaticMarkup(<MatchedCount count={0} />);
    expect(html).toContain("text-red-600");
    expect(html).toContain(">0</span>");
  });

  it("有命中时正常显示", () => {
    const html = renderToStaticMarkup(<MatchedCount count={1406} />);
    expect(html).toContain("1406");
    expect(html).not.toContain("text-red-600");
  });
});

describe("validatePrices", () => {
  it("accepts zero and positive prices", () => {
    expect(validatePrices("0", "0", "0", "0")).toBeNull();
    expect(validatePrices("0.15", "0.6", "0.003", "1")).toBeNull();
  });

  it("rejects negative prices with a Chinese field-specific message", () => {
    expect(validatePrices("-1", "0", "0", "0")).toContain("输入");
    expect(validatePrices("0", "-1", "0", "0")).toContain("输出");
    expect(validatePrices("0", "0", "-1", "0")).toContain("缓存读取");
    expect(validatePrices("0", "0", "0", "-1")).toContain("缓存创建");
    expect(validatePrices("0", "0", "0", "-1")).toContain("不能为负");
  });

  it("rejects empty and non-numeric prices", () => {
    expect(validatePrices("", "0", "0", "0")).toContain("不能为空");
    expect(validatePrices("abc", "0", "0", "0")).toContain("有效数字");
  });
});
