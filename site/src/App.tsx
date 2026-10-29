import { FluentProvider, makeStyles, tokens, webDarkTheme, webLightTheme } from "@fluentui/react-components";
import { useEffect, useState } from "react";
import { SiteFooter, SiteHeader } from "./chrome";
import { chrome, meta } from "./copy";
import { useRoute } from "./navigation";
import { Home } from "./pages/Home";
import { Privacy } from "./pages/Privacy";
import { Security } from "./pages/Security";
import { hrefFor, siteOrigin } from "./router";
import type { Route } from "./router";

const useStyles = makeStyles({
  root: {
    minHeight: "100vh",
    backgroundColor: tokens.colorNeutralBackground1,
    color: tokens.colorNeutralForeground1,
  },
  main: { outlineStyle: "none" },
});

/** The theme follows the system, like the launcher itself. */
function useSystemDark(): boolean {
  const query = "(prefers-color-scheme: dark)";
  const [dark, setDark] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const media = window.matchMedia(query);
    const listener = (event: MediaQueryListEvent) => setDark(event.matches);
    media.addEventListener("change", listener);
    return () => media.removeEventListener("change", listener);
  }, []);
  return dark;
}

function setMeta(selector: string, create: () => HTMLElement, attribute: string, value: string) {
  const element = document.head.querySelector<HTMLElement>(selector) ?? create();
  element.setAttribute(attribute, value);
  if (!element.isConnected) {
    document.head.appendChild(element);
  }
}

/** Keeps the document title, language, description and canonical link in step with the page. */
function useDocumentHead(route: Route) {
  useEffect(() => {
    const page = meta[route.lang][route.page];
    document.title = page.title;
    document.documentElement.lang = chrome[route.lang].htmlLang;
    setMeta('meta[name="description"]', () => Object.assign(document.createElement("meta"), { name: "description" }), "content", page.description);
    setMeta('link[rel="canonical"]', () => Object.assign(document.createElement("link"), { rel: "canonical" }), "href", siteOrigin + hrefFor(route));
  }, [route]);
}

function Page({ route }: { route: Route }) {
  switch (route.page) {
    case "security":
      return <Security lang={route.lang} />;
    case "privacy":
      return <Privacy lang={route.lang} />;
    default:
      return <Home lang={route.lang} />;
  }
}

export default function App() {
  const styles = useStyles();
  const route = useRoute();
  const dark = useSystemDark();
  useDocumentHead(route);

  return (
    <FluentProvider theme={dark ? webDarkTheme : webLightTheme} className={styles.root}>
      <SiteHeader route={route} />
      <main id="main" tabIndex={-1} className={styles.main}>
        <Page route={route} />
      </main>
      <SiteFooter route={route} />
    </FluentProvider>
  );
}
