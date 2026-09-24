import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { PricingOverrideDialog, validatePrices } from "./PricingOverrideDialog";

describe("PricingOverrideDialog", () => {
  it("renders provider options, four price inputs and save button", () => {
    const html = renderToStaticMarkup(
      <PricingOverrideDialog providers={["anthropic", "openai"]} onClose={() => {}} onChanged={() => {}} />,
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
