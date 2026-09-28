import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  ScopePicker, ScopeChips, toggleProvider, toggleModel, selectedModelCount, scopeSummary,
} from "./ScopePicker";
import { ScopeFilter } from "../lib/api";

const GROUPS = [
  { provider_id: "p1", models: ["m1", "m2"] },
  { provider_id: "p2", models: ["m3"] },
];

const EMPTY: ScopeFilter = { providers: [], models: [] };

describe("toggleProvider", () => {
  it("selects the whole provider and drops its model-level picks", () => {
    const sel: ScopeFilter = { providers: [], models: [{ provider_id: "p1", model_id: "m1" }] };
    const next = toggleProvider(sel, "p1");
    expect(next.providers).toEqual(["p1"]);
    // 供应商级选中时其模型级选择应移除，避免重复
    expect(next.models).toEqual([]);
  });

  it("deselects the provider", () => {
    const sel: ScopeFilter = { providers: ["p1"], models: [] };
    expect(toggleProvider(sel, "p1").providers).toEqual([]);
  });
});

describe("toggleModel", () => {
  it("adds a model pick and unchecks the provider-level selection", () => {
    const sel: ScopeFilter = { providers: ["p1"], models: [] };
    const next = toggleModel(sel, "p1", "m1");
    // 改为精确到模型时供应商级选择应取消
    expect(next.providers).toEqual([]);
    expect(next.models).toEqual([{ provider_id: "p1", model_id: "m1" }]);
  });

  it("removes an existing model pick", () => {
    const sel: ScopeFilter = { providers: [], models: [{ provider_id: "p1", model_id: "m1" }] };
    expect(toggleModel(sel, "p1", "m1").models).toEqual([]);
  });
});

describe("selectedModelCount and scopeSummary", () => {
  it("counts per-provider model picks", () => {
    const sel: ScopeFilter = {
      providers: ["p2"],
      models: [{ provider_id: "p1", model_id: "m1" }, { provider_id: "p1", model_id: "m2" }],
    };
    expect(selectedModelCount(sel, "p1")).toBe(2);
    expect(selectedModelCount(sel, "p2")).toBe(0);
    expect(scopeSummary(sel)).toBe("1 个供应商（全部模型） + 2 个指定模型");
  });

  it("describes empty scope as unfiltered", () => {
    expect(scopeSummary(EMPTY)).toBe("全部（未筛选）");
  });
});

describe("ScopePicker rendering", () => {
  it("renders provider rows with expand buttons and empty state when no data", () => {
    const html = renderToStaticMarkup(
      <ScopePicker groups={GROUPS} selected={EMPTY} onChange={() => {}} />,
    );
    expect(html).toContain("p1");
    expect(html).toContain("p2");
    expect(html).toContain("展开 2 个模型");
    const empty = renderToStaticMarkup(
      <ScopePicker groups={[]} selected={EMPTY} onChange={() => {}} />,
    );
    expect(empty).toContain("暂无用量数据");
  });
});

describe("ScopeChips rendering", () => {
  it("shows provider and model chips with a clear button", () => {
    const sel: ScopeFilter = {
      providers: ["p1"],
      models: [{ provider_id: "p2", model_id: "m3" }],
    };
    const html = renderToStaticMarkup(
      <ScopeChips selected={sel} onRemoveProvider={() => {}} onRemoveModel={() => {}} onClear={() => {}} />,
    );
    expect(html).toContain("p1（全部模型）");
    expect(html).toContain("p2 › m3");
    expect(html).toContain("清除筛选");
  });

  it("renders nothing for empty scope", () => {
    const html = renderToStaticMarkup(
      <ScopeChips selected={EMPTY} onRemoveProvider={() => {}} onRemoveModel={() => {}} onClear={() => {}} />,
    );
    expect(html).toBe("");
  });
});
