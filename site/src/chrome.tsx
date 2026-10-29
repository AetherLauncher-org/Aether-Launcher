import { Button, Link, Text, makeStyles, tokens } from "@fluentui/react-components";
import type { ReactNode } from "react";
import { chrome } from "./copy";
import { navigate } from "./navigation";
import { hrefFor, otherLanguage, repositoryUrl } from "./router";
import type { PageId, Route } from "./router";

const useStyles = makeStyles({
  skip: {
    position: "absolute",
    top: "-48px",
    left: "16px",
    zIndex: 100,
    padding: "8px 16px",
    borderRadius: tokens.borderRadiusMedium,
    backgroundColor: tokens.colorNeutralBackground1,
    border: `1px solid ${tokens.colorNeutralStroke1}`,
    transitionProperty: "top",
    transitionDuration: tokens.durationFast,
    ":focus": { top: "12px" },
  },
  header: {
    position: "sticky",
    top: 0,
    zIndex: 50,
    // The same translucent bar the launcher's title bar uses.
    backgroundColor: "color-mix(in srgb, var(--colorNeutralBackground2) 72%, transparent)",
    backdropFilter: "saturate(180%) blur(24px)",
    borderBottom: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  headerInner: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    columnGap: "16px",
    minHeight: "56px",
    maxWidth: "1080px",
    marginInline: "auto",
    paddingInline: "16px",
  },
  brand: {
    display: "inline-flex",
    alignItems: "center",
    columnGap: "10px",
    color: tokens.colorNeutralForeground1,
    textDecorationLine: "none",
    fontFamily: tokens.fontFamilyBase,
    fontSize: tokens.fontSizeBase400,
    fontWeight: tokens.fontWeightSemibold,
    ":hover": { textDecorationLine: "none" },
  },
  logo: { width: "28px", height: "28px" },
  nav: {
    display: "flex",
    flexGrow: 1,
    flexWrap: "wrap",
    alignItems: "center",
    justifyContent: "flex-end",
    columnGap: "4px",
  },
  current: {
    boxShadow: `inset 0 -2px 0 ${tokens.colorCompoundBrandStroke}`,
    fontWeight: tokens.fontWeightSemibold,
  },
  footer: {
    marginTop: "40px",
    paddingBlock: "32px 48px",
    borderTop: `1px solid ${tokens.colorNeutralStroke2}`,
  },
  footerInner: {
    display: "grid",
    rowGap: "8px",
    maxWidth: "1080px",
    marginInline: "auto",
    paddingInline: "16px",
  },
});

type SiteLinkProps = { route: Route; children: ReactNode; className?: string };

/** A real link that navigates without a reload on a plain click. */
export function SiteLink({ route, children, className }: SiteLinkProps) {
  const href = hrefFor(route);
  return (
    <Link
      href={href}
      className={className}
      onClick={(event) => {
        if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
          return;
        }
        event.preventDefault();
        navigate(href);
      }}
    >
      {children}
    </Link>
  );
}

/** A Fluent button that is really an anchor, with the same in-page navigation. */
export function ButtonLink({
  route,
  appearance,
  size,
  children,
  current,
  className,
}: {
  route: Route;
  appearance?: "primary" | "secondary" | "subtle" | "outline";
  size?: "small" | "medium" | "large";
  children: ReactNode;
  current?: boolean;
  className?: string;
}) {
  const href = hrefFor(route);
  return (
    <Button
      as="a"
      href={href}
      appearance={appearance}
      size={size}
      className={className}
      aria-current={current ? "page" : undefined}
      onClick={(event) => {
        if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
          return;
        }
        event.preventDefault();
        navigate(href);
      }}
    >
      {children}
    </Button>
  );
}

export function SiteHeader({ route }: { route: Route }) {
  const styles = useStyles();
  const text = chrome[route.lang];
  const items: { page: PageId; label: string }[] = [
    { page: "home", label: text.nav.home },
    { page: "security", label: text.nav.security },
    { page: "privacy", label: text.nav.privacy },
  ];
  const other = otherLanguage(route);

  return (
    <header className={styles.header}>
      <a
        className={styles.skip}
        href="#main"
        onClick={(event) => {
          event.preventDefault();
          document.getElementById("main")?.focus();
        }}
      >
        {text.skip}
      </a>
      <div className={styles.headerInner}>
        <a
          className={styles.brand}
          href={hrefFor({ lang: route.lang, page: "home" })}
          onClick={(event) => {
            event.preventDefault();
            navigate(hrefFor({ lang: route.lang, page: "home" }));
          }}
        >
          <img className={styles.logo} src="/logo.svg" alt="" width={28} height={28} />
          Aether Launcher
        </a>
        <nav className={styles.nav} aria-label={text.menuLabel}>
          {items.map((item) => (
            <ButtonLink
              key={item.page}
              route={{ lang: route.lang, page: item.page }}
              appearance="subtle"
              current={item.page === route.page}
              className={item.page === route.page ? styles.current : undefined}
            >
              {item.label}
            </ButtonLink>
          ))}
          <Button as="a" appearance="subtle" href={repositoryUrl} rel="noopener">
            {text.nav.github}
          </Button>
          <ButtonLink route={other} appearance="outline">
            <span lang={other.lang === "zh" ? "zh-CN" : "en"}>{text.switchTo}</span>
          </ButtonLink>
        </nav>
      </div>
    </header>
  );
}

export function SiteFooter({ route }: { route: Route }) {
  const styles = useStyles();
  const text = chrome[route.lang];
  return (
    <footer className={styles.footer}>
      <div className={styles.footerInner}>
        <Text size={200}>{text.footerDisclaimer}</Text>
        <Text size={200}>
          {text.footerLicense.before}
          <Link href={repositoryUrl} rel="noopener">
            GitHub
          </Link>
          {text.footerLicense.after}
        </Text>
      </div>
    </footer>
  );
}
