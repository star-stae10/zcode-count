import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { AuditLogDialog, ModelCell, ProviderCell } from "./AuditLogDialog";
import { AuditLogRow } from "../lib/api";

// AuditLogDialog 在 useEffect 中拉取数据；SSR 不跑 effect，rows 保持 null（加载态），
// 表头仅在拿到数据后渲染。此处验证加载态与说明文案；行内供应商/模型列单独经 ProviderCell/ModelCell 验证。
describe("AuditLogDialog", () => {
  it("renders header, hint and loading state initially", () => {
    const html = renderToStaticMarkup(<AuditLogDialog names={{}} onClose={() => {}} />);
    expect(html).toContain("清除操作审计记录");
    expect(html).toContain("最多显示最近 100 条清除操作记录");
    expect(html).toContain("加载中");
    // 表头仅在数据就绪后渲染
    expect(html).not.toContain("操作人");
  });

  it("row type carries actor, scope and deleted count fields", () => {
    const r: AuditLogRow = {
      id: 1, actor: "fufu", action: "clear_usage",
      started_at_from: 0, started_at_to: 100,
      providers: ["p1"],
      models: [{ provider_id: "p2", model_id: "m3" }],
      deleted_count: 5, created_at: 0,
    };
    expect(r.deleted_count).toBe(5);
    expect(r.models[0].model_id).toBe("m3");
  });
});

describe("ProviderCell / ModelCell (audit row scope columns)", () => {
  it("shows mapped names and keeps the raw provider_id list in title", () => {
    const html = renderToStaticMarkup(
      <ProviderCell names={{ p1: "OpenCode Go" }} providers={["p1", "raw-id"]} />,
    );
    expect(html).toContain("OpenCode Go、raw-id");
    expect(html).toContain('title="p1、raw-id"');
  });

  it("falls back to raw ids without title when nothing is mapped; empty list shows 全部", () => {
    const html = renderToStaticMarkup(<ProviderCell names={{}} providers={["p1"]} />);
    expect(html).toContain("p1");
    expect(html).not.toContain("title=");
    const all = renderToStaticMarkup(<ProviderCell names={{}} providers={[]} />);
    expect(all).toContain("全部");
  });

  it("shows model rows with mapped provider names, raw ids in title", () => {
    const html = renderToStaticMarkup(
      <ModelCell names={{ p2: "Start Plan" }} models={[{ provider_id: "p2", model_id: "m3" }]} />,
    );
    expect(html).toContain("Start Plan › m3");
    expect(html).toContain('title="p2 › m3"');
    const all = renderToStaticMarkup(<ModelCell names={{}} models={[]} />);
    expect(all).toContain("全部模型");
  });
});
