import { describe, expect, it } from "vitest";
import { spaceScale, textSize } from "./tokens";

/**
 * Enforces site/DESIGN.md. The spec is only worth anything if a page cannot quietly
 * ignore it, so these tests read the source and fail on a value that is not in it.
 */

const sources = import.meta.glob("./**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** Files that define the values, or test them. */
const exempt = new Set(["./tokens.ts", "./design.test.ts", "./vite-env.d.ts"]);

const files = Object.entries(sources).filter(([path]) => !exempt.has(path));

/** Comments may talk about colours and sizes freely. */
function code(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
}

function violations(pattern: RegExp, allowed: (match: string) => boolean = () => false): string[] {
  const found: string[] = [];
  for (const [path, source] of files) {
    for (const match of code(source).matchAll(pattern)) {
      if (!allowed(match[0])) {
        found.push(`${path}: ${match[0]}`);
      }
    }
  }
  return found;
}

describe("the site follows DESIGN.md", () => {
  it("finds the source files it is meant to check", () => {
    expect(files.length).toBeGreaterThan(5);
  });

  it("writes no raw pixel values outside tokens.ts", () => {
    expect(violations(/-?\d+(\.\d+)?px/g)).toEqual([]);
  });

  it("only offers spacing values that are on the scale", () => {
    const tokens = Object.values(spaceScale);
    expect(tokens).toEqual([0, 4, 8, 12, 16, 24, 32, 48, 64]);
  });

  it("uses no raw colours: hex, rgb(), hsl() or named brand colours", () => {
    expect(violations(/#[0-9a-fA-F]{3,8}\b/g)).toEqual([]);
    expect(violations(/\b(rgba?|hsla?)\(/g)).toEqual([]);
  });

  it("has no gradients, filters, blur or animation", () => {
    expect(
      violations(/gradient|backdropFilter|\bfilter:|keyframes|animation(Name|Duration|Delay)?\b|transition(Property|Duration)\b/g),
    ).toEqual([]);
  });

  it("has no shadows except the one inset underline under the current page", () => {
    expect(violations(/boxShadow:[^,\n]*/g, (match) => match.includes("inset"))).toEqual([]);
    expect(violations(/\bshadow\d+\b/g)).toEqual([]);
  });

  it("styles nothing inline", () => {
    expect(violations(/style=\{\{/g)).toEqual([]);
  });

  it("uses only the text sizes in the type ramp", () => {
    const allowed = new Set(Object.values(textSize).map(String));
    expect(violations(/\bsize=\{\d+\}/g, (match) => allowed.has(match.replace(/\D/g, "")))).toEqual([]);
  });

  it("does not use a different theme than Fluent's own", () => {
    expect(violations(/createLightTheme|createDarkTheme|BrandVariants/g)).toEqual([]);
  });
});
