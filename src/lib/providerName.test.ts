import { describe, it, expect } from "vitest";
import { hasAnyName, providerLabel, providerListLabel } from "./providerName";

const NAMES = { "opencode-go-chat": "OpenCode Go", "new-provider": "旧供应商" };

describe("providerLabel", () => {
  it("returns the mapped display name when present", () => {
    expect(providerLabel(NAMES, "opencode-go-chat")).toBe("OpenCode Go");
  });

  it("falls back to the raw provider_id when unmapped", () => {
    expect(providerLabel(NAMES, "07d35cbb-f016-43b1-94fe-301db6e709cf")).toBe(
      "07d35cbb-f016-43b1-94fe-301db6e709cf",
    );
    expect(providerLabel(NAMES, "builtin:zai-start-plan")).toBe("builtin:zai-start-plan");
  });

  it("falls back on an empty mapping", () => {
    expect(providerLabel({}, "p1")).toBe("p1");
  });
});

describe("providerListLabel", () => {
  it("joins mapped names in order", () => {
    expect(providerListLabel(NAMES, ["opencode-go-chat", "x"], "全部")).toBe("OpenCode Go、x");
  });

  it("returns the empty copy for an empty list", () => {
    expect(providerListLabel(NAMES, [], "全部")).toBe("全部");
  });
});

describe("hasAnyName", () => {
  it("detects whether any id has a mapping (drives title output)", () => {
    expect(hasAnyName(NAMES, ["opencode-go-chat", "x"])).toBe(true);
    expect(hasAnyName(NAMES, ["x", "y"])).toBe(false);
    expect(hasAnyName(NAMES, [])).toBe(false);
  });
});
