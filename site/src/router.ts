/**
 * A tiny path router. The site has eight pages (four, in two languages), so a library
 * would cost more than it saves. Every page is a real URL and every link is a real
 * `<a href>`, so open-in-new-tab, copy-link and the back button all behave.
 */

export type Lang = "en" | "zh";
export type PageId = "home" | "security" | "privacy" | "terms";
export type Route = { lang: Lang; page: PageId };

export const siteOrigin = "https://launcher.abnt.it";
export const repositoryUrl = "https://github.com/AetherLauncher-org/Aether-Launcher";
/** Shown on every page that states a date. Bump it when the content changes. */
export const lastUpdated = "2026-10-30";

const pagePaths: Record<PageId, string> = {
  home: "",
  security: "security",
  privacy: "privacy",
  terms: "terms",
};

export function parseRoute(pathname: string): Route {
  const parts = pathname.split("/").filter((part) => part.length > 0);
  const lang: Lang = parts[0] === "zh" ? "zh" : "en";
  const rest = lang === "zh" ? parts.slice(1) : parts;
  const page = (Object.keys(pagePaths) as PageId[]).find((id) => pagePaths[id] === (rest[0] ?? ""));
  return { lang, page: page ?? "home" };
}

export function hrefFor(route: Route): string {
  const prefix = route.lang === "zh" ? "/zh" : "";
  const path = pagePaths[route.page];
  if (path === "") {
    return prefix === "" ? "/" : `${prefix}/`;
  }
  return `${prefix}/${path}`;
}

export function otherLanguage(route: Route): Route {
  return { ...route, lang: route.lang === "en" ? "zh" : "en" };
}

export function allRoutes(): Route[] {
  const pages = Object.keys(pagePaths) as PageId[];
  return (["en", "zh"] as Lang[]).flatMap((lang) => pages.map((page) => ({ lang, page })));
}
