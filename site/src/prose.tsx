import { MessageBar, MessageBarBody, Text, makeStyles, tokens } from "@fluentui/react-components";
import type { ReactNode } from "react";
import { screen, space, textSize, widths } from "./tokens";

const useStyles = makeStyles({
  article: {
    display: "grid",
    rowGap: space.m,
    // The same 960 container as the header, with the text in a left-aligned reading column.
    gridTemplateColumns: `minmax(0, ${widths.prose})`,
    maxWidth: widths.page,
    marginInline: "auto",
    paddingTop: space.page,
    paddingInline: space.xl,
    [screen.phone]: { paddingInline: space.l },
  },
  header: { display: "grid", rowGap: space.s },
  meta: { color: tokens.colorNeutralForeground3 },
  h2: { marginTop: space.xl },
  p: {
    color: tokens.colorNeutralForeground2,
    lineHeight: tokens.lineHeightBase400,
  },
  list: {
    display: "grid",
    rowGap: space.s,
    margin: 0,
    paddingLeft: space.xl,
    color: tokens.colorNeutralForeground2,
    fontSize: tokens.fontSizeBase400,
    lineHeight: tokens.lineHeightBase400,
  },
  code: {
    paddingInline: space.xs,
    borderRadius: tokens.borderRadiusSmall,
    backgroundColor: tokens.colorNeutralBackground3,
    fontFamily: tokens.fontFamilyMonospace,
    fontSize: tokens.fontSizeBase300,
    overflowWrap: "anywhere",
  },
});

export function Article({ title, meta, children }: { title: string; meta: ReactNode; children: ReactNode }) {
  const styles = useStyles();
  return (
    <article className={styles.article}>
      <div className={styles.header}>
        <Text as="h1" size={textSize.title} weight="semibold">
          {title}
        </Text>
        <Text size={textSize.ui} className={styles.meta}>
          {meta}
        </Text>
      </div>
      {children}
    </article>
  );
}

export function H2({ children }: { children: ReactNode }) {
  const styles = useStyles();
  return (
    <Text as="h2" size={textSize.section} weight="semibold" className={styles.h2}>
      {children}
    </Text>
  );
}

export function P({ children }: { children: ReactNode }) {
  const styles = useStyles();
  return (
    <Text as="p" size={textSize.body} className={styles.p}>
      {children}
    </Text>
  );
}

export function UL({ children }: { children: ReactNode }) {
  const styles = useStyles();
  return <ul className={styles.list}>{children}</ul>;
}

export function Code({ children }: { children: ReactNode }) {
  const styles = useStyles();
  return <code className={styles.code}>{children}</code>;
}

export function Callout({ children }: { children: ReactNode }) {
  return (
    <MessageBar intent="info">
      <MessageBarBody>{children}</MessageBarBody>
    </MessageBar>
  );
}
