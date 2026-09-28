import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ClearDataDialog, ModelList, validateClearForm } from "./ClearDataDialog";

describe("validateClearForm", () => {
  it("passes when time range is not enabled", () => {
    expect(validateClearForm(false, "", "")).toBeNull();
  });

  it("requires both dates when time range is enabled", () => {
    expect(validateClearForm(true, "", "2026-01-02")).toContain("必须填写");
    expect(validateClearForm(true, "2026-01-01", "")).toContain("必须填写");
  });

  it("rejects start after end", () => {
    expect(validateClearForm(true, "2026-01-03", "2026-01-01")).toContain("晚于结束日期");
    expect(validateClearForm(true, "2026-01-01", "2026-01-03")).toBeNull();
  });
});

describe("ClearDataDialog rendering", () => {
  it("labels the danger, distinguishes from scope filtering and guards full clear", () => {
    const html = renderToStaticMarkup(
      <ClearDataDialog providerModels={[]} names={{}} onClose={() => {}} onDone={() => {}} onOpenAudit={() => {}} />,
    );
    expect(html).toContain("清除用量数据（永久删除，不可恢复）");
    expect(html).toContain("与「核算范围」不同：核算范围只影响展示，不删除数据");
    // 无任何条件时应明确警示全清
    expect(html).toContain("将清除全部数据");
    expect(html).toContain("暂无用量数据");
    // 预览前不应出现执行按钮
    expect(html).not.toContain("永久删除</button>");
  });

  it("shows preview step UI only after preview (no confirm input initially)", () => {
    const html = renderToStaticMarkup(
      <ClearDataDialog providerModels={[]} names={{}} onClose={() => {}} onDone={() => {}} onOpenAudit={() => {}} />,
    );
    expect(html).toContain("预览将删除的数据");
    expect(html).not.toContain("以授权");
  });
});

describe("ModelList (clear scope model list)", () => {
  it("shows mapped provider names with raw provider_id kept in title", () => {
    const html = renderToStaticMarkup(
      <ModelList names={{ "new-provider": "OpenCode Go" }} models={[{ provider_id: "new-provider", model_id: "m1" }]} />,
    );
    expect(html).toContain("OpenCode Go › m1");
    expect(html).toContain('title="new-provider"');
    expect(html).not.toContain(">new-provider<");
  });

  it("falls back to raw ids without title when unmapped; empty list shows the unbounded copy", () => {
    const html = renderToStaticMarkup(
      <ModelList names={{}} models={[{ provider_id: "p1", model_id: "m1" }]} />,
    );
    expect(html).toContain("p1 › m1");
    expect(html).not.toContain("title=");
    const none = renderToStaticMarkup(<ModelList names={{}} models={[]} />);
    expect(none).toContain("不限（所选供应商的全部模型）");
  });
});
