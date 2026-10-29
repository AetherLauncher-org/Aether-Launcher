import { MessageBar, MessageBarBody, Text, makeStyles, tokens } from "@fluentui/react-components";
import type { ReactNode } from "react";

const useStyles = makeStyles({
  article: {
    maxWidth: "760px",
    marginInline: "auto",
    paddingBlock: "48px 24px",
    paddingInline: "16px",
  },
  title: { display: "block", marginBottom: "8px" },
  meta: { display: "block", color: tokens.colorNeutralForeground3 },
  h2: { display: "block", marginTop: "40px", marginBottom: "8px" },
  p: {
    display: "block",
    marginBlock: "12px",
    color: tokens.colorNeutralForeground2,
    lineHeight: tokens.lineHeightBase400,
  },
  list: {
    marginBlock: "12px",
    paddingLeft: "22px",
    color: tokens.colorNeutralForeground2,
    fontSize: tokens.fontSizeBase300,
    lineHeight: tokens.lineHeightBase400,
    "> li + li": { marginTop: "6px" },
  },
  code: {
    paddingBlock: "1px",
    paddingInline: "6px",
    borderRadius: tokens.borderRadiusSmall,
    backgroundColor: tokens.colorBrandBackground2,
    fontFamily: tokens.fontFamilyMonospace,
    fontSize: "13px",
    overflowWrap: "anywhere",
  },
  callout: { marginBlock: "20px" },
});

export function Article({ title, meta, children }: { title: string; meta: ReactNode; children: ReactNode }) {
  const styles = useStyles();
  return (
    <article className={styles.article}>
      <Text as="h1" size={900} weight="semibold" className={styles.title}>
        {title}
      </Text>
      <Text size={300} className={styles.meta}>
        {meta}
      </Text>
      {children}
    </article>
  );
}

export function H2({ children }: { children: ReactNode }) {
  const styles = useStyles();
  return (
    <Text as="h2" size={600} weight="semibold" className={styles.h2}>
      {children}
    </Text>
  );
}

export function P({ children }: { children: ReactNode }) {
  const styles = useStyles();
  return (
    <Text as="p" size={300} className={styles.p}>
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
  const styles = useStyles();
  return (
    <MessageBar className={styles.callout} intent="info">
      <MessageBarBody>{children}</MessageBarBody>
    </MessageBar>
  );
}
