// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { renderMarkdown } from "./markdown";

describe("renderMarkdown", () => {
  it("renders basic formatting", () => {
    const html = renderMarkdown("**Bold** and a list:\n\n- one\n- two");
    expect(html).toContain("<strong>Bold</strong>");
    expect(html).toContain("<li>one</li>");
  });

  it("highlights code and adds a copy button", () => {
    const html = renderMarkdown("```python\nprint('hi')\n```");
    expect(html).toContain('class="hljs"');
    expect(html).toContain("data-copy");
    expect(html).toContain("python");
  });

  it("closes an unfinished code block while a reply is still streaming", () => {
    const html = renderMarkdown("Here:\n```js\nconst x = 1");
    expect(html).toContain("<pre");
  });

  it("strips anything dangerous a model might output", () => {
    const html = renderMarkdown('<img src=x onerror="alert(1)"><script>alert(2)</script>[x](javascript:alert(3))');
    expect(html).not.toContain("onerror");
    expect(html).not.toContain("<script");
    expect(html).not.toContain("javascript:");
  });
});
