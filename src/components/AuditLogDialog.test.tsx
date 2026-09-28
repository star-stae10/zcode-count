import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { AuditLogDialog } from "./AuditLogDialog";
import { AuditLogRow } from "../lib/api";

// AuditLogDialog 在 useEffect 中拉取数据；SSR 不跑 effect，rows 保持 null（加载态），
// 表头仅在拿到数据后渲染。此处验证加载态与说明文案。
describe("AuditLogDialog", () => {
  it("renders header, hint and loading state initially", () => {
    const html = renderToStaticMarkup(<AuditLogDialog onClose={() => {}} />);
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
