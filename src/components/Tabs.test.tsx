import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { Tabs } from "./Tabs";

describe("Tabs", () => {
  it("renders all three tab labels", () => {
    const html = renderToStaticMarkup(<Tabs active="logs" onChange={() => {}} />);
    expect(html).toContain("请求日志");
    expect(html).toContain("Provider 统计");
    expect(html).toContain("模型统计");
  });

  it("highlights the active tab", () => {
    const html = renderToStaticMarkup(<Tabs active="providers" onChange={() => {}} />);
    expect(html).toContain("text-blue-600");
    expect(html).toContain("font-medium");
    expect(html).toContain("text-gray-500");
  });
});
