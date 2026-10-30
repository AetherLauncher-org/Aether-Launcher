import { Button, Link, Text, makeStyles, tokens } from "@fluentui/react-components";
import type { ReactNode } from "react";
import { chrome } from "./copy";
import { navigate } from "./navigation";
import { hrefFor, otherLanguage, repositoryUrl } from "./router";
import type { PageId, Route } from "./router";
import { screen, sizes, space, textSize, widths } from "./tokens";

const useStyles = makeStyles({
  skip: {
    position: "absolute",
    top: "-100vh",
    left: space.l,
    zIndex: 100,
    paddingBlock: space.s,
    paddingInline: space.l,
    borderRadius: tokens.borderRadiusMedium,
    backgroundColor: tokens.colorNeutralBackground1,
    border: `${sizes.hairline} solid ${tokens.colorNeutralStroke1}`,
    ":focus": { top: space.m },
  },
  // Transparent: the backdrop runs behind the header, so the page reads as one piece.
  header: {},
  headerInner: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    columnGap: space.l,
    minHeight: sizes.header,
    maxWidth: widths.page,
    marginInline: "auto",
    paddingInline: space.xl,
    [screen.phone]: { paddingInline: space.l, paddingBlock: space.s },
  },
  brand: {
    display: "inline-flex",
    alignItems: "center",
    columnGap: space.m,
    color: tokens.colorNeutralForeground1,
    textDecorationLine: "none",
    fontFamily: tokens.fontFamilyBase,
    fontSize: tokens.fontSizeBase400,
    fontWeight: tokens.fontWeightSemibold,
    ":hover": { textDecorationLine: "none" },
  },
  logo: { width: sizes.headerLogo, height: sizes.headerLogo },
  nav: {
    display: "flex",
    flexGrow: 1,
    flexWrap: "wrap",
    alignItems: "center",
    justifyContent: "flex-end",
    columnGap: space.xs,
    [screen.phone]: { justifyContent: "flex-start" },
  },
  current: {
    boxShadow: `inset 0 -${sizes.indicator} 0 ${tokens.colorCompoundBrandStroke}`,
    fontWeight: tokens.fontWeightSemibold,
    borderRadius: tokens.borderRadiusNone,
  },
  footer: {
    marginTop: space.section,
    paddingBlock: `${space.xxl} ${space.section}`,
    borderTop: `${sizes.hairline} solid ${tokens.colorNeutralStroke2}`,
  },
  footerInner: {
    display: "grid",
    rowGap: space.s,
    maxWidth: widths.page,
    marginInline: "auto",
    paddingInline: space.xl,
    color: tokens.colorNeutralForeground3,
    [screen.phone]: { paddingInline: space.l },
  },
  footerText: { maxWidth: widths.prose },
});

type SiteLinkProps = { route: Route; children: ReactNode; className?: string };

function isPlainClick(event: { button: number; metaKey: boolean; ctrlKey: boolean; shiftKey: boolean; altKey: boolean }) {
  return event.button === 0 && !event.metaKey && !event.ctrlKey && !event.shiftKey && !event.altKey;
}

/** A real link that navigates without a reload on a plain click. */
export function SiteLink({ route, children, className }: SiteLinkProps) {
  const href = hrefFor(route);
  return (
    <Link
      href={href}
      className={className}
      onClick={(event) => {
        if (isPlainClick(event)) {
          event.preventDefault();
          navigate(href);
        }
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
  children,
  current,
  className,
}: {
  route: Route;
  appearance?: "primary" | "secondary" | "subtle" | "outline";
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
      className={className}
      aria-current={current ? "page" : undefined}
      onClick={(event) => {
        if (isPlainClick(event)) {
          event.preventDefault();
          navigate(href);
        }
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
    { page: "terms", label: text.nav.terms },
  ];
  const other = otherLanguage(route);
  const homeHref = hrefFor({ lang: route.lang, page: "home" });

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
          href={homeHref}
          onClick={(event) => {
            event.preventDefault();
            navigate(homeHref);
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
          <ButtonLink route={other} appearance="subtle">
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
        <Text size={textSize.caption} className={styles.footerText}>
          {text.footerDisclaimer}
        </Text>
        <Text size={textSize.caption} className={styles.footerText}>
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
