import { Badge, Button, Card, Text, makeStyles, tokens } from "@fluentui/react-components";
import {
  ArrowDownload24Regular,
  Code24Regular,
  Folder24Regular,
  Gauge24Regular,
  Person24Regular,
  ShieldCheckmark24Regular,
} from "@fluentui/react-icons";
import type { ReactNode } from "react";
import { ButtonLink } from "../chrome";
import { lastUpdated, repositoryUrl } from "../router";
import type { Lang } from "../router";
import { screen, sizes, space, textSize, widths } from "../tokens";

const useStyles = makeStyles({
  container: {
    maxWidth: widths.page,
    marginInline: "auto",
    paddingInline: space.xl,
    [screen.phone]: { paddingInline: space.l },
  },
  hero: {
    display: "grid",
    rowGap: space.l,
    justifyItems: "start",
    paddingTop: space.page,
  },
  heroLogo: { width: sizes.heroLogo, height: sizes.heroLogo },
  lead: { maxWidth: widths.prose, color: tokens.colorNeutralForeground2 },
  actions: { display: "flex", flexWrap: "wrap", columnGap: space.m, rowGap: space.m },
  caption: { color: tokens.colorNeutralForeground3 },
  section: {
    display: "grid",
    rowGap: space.l,
    marginTop: space.section,
  },
  sectionLead: { maxWidth: widths.prose, color: tokens.colorNeutralForeground2 },
  grid: {
    display: "grid",
    gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
    columnGap: space.l,
    rowGap: space.l,
    [screen.tablet]: { gridTemplateColumns: "repeat(2, minmax(0, 1fr))" },
    [screen.phone]: { gridTemplateColumns: "minmax(0, 1fr)" },
  },
  card: {
    alignItems: "flex-start",
    rowGap: space.m,
    padding: space.xl,
    backgroundColor: tokens.colorNeutralBackground1,
  },
  icon: { color: tokens.colorNeutralForeground2 },
  cardBody: { color: tokens.colorNeutralForeground2 },
  steps: {
    display: "grid",
    margin: 0,
    paddingBlock: space.s,
    paddingInline: space.xl,
    listStyleType: "none",
    backgroundColor: tokens.colorNeutralBackground1,
    border: `${sizes.hairline} solid ${tokens.colorNeutralStroke2}`,
    borderRadius: tokens.borderRadiusXLarge,
  },
  step: {
    display: "grid",
    gridTemplateColumns: `${sizes.badgeColumn} minmax(0, 1fr)`,
    alignItems: "start",
    columnGap: space.l,
    paddingBlock: space.m,
    borderBottom: `${sizes.hairline} solid ${tokens.colorNeutralStroke2}`,
    ":last-child": { borderBottomStyle: "none" },
    [screen.phone]: { gridTemplateColumns: "minmax(0, 1fr)", rowGap: space.s },
  },
  stepText: { display: "grid", rowGap: space.xs },
  stepDetail: { color: tokens.colorNeutralForeground2 },
});

type Feature = { icon: ReactNode; title: string; body: string };
type Step = { tone: "done" | "wip" | "todo"; label: string; name: string; detail: string };

type Copy = {
  status: string;
  lead: string;
  primary: string;
  secondary: string;
  featuresTitle: string;
  featuresLead: string;
  features: Feature[];
  statusTitle: string;
  statusLead: string;
  steps: Step[];
};

const en: Copy = {
  status: "Early development: no public release yet.",
  lead: "A transparent, resilient Minecraft: Java Edition launcher built with Rust, Tauri 2 and Fluent 2.",
  primary: "View on GitHub",
  secondary: "Read the security model",
  featuresTitle: "What it does",
  featuresLead: "Aether is built around a few promises that you can read in the source.",
  features: [
    {
      icon: <ShieldCheckmark24Regular />,
      title: "Verified downloads",
      body: "Every game file is checked against the SHA-1 in Mojang’s official manifest before it is used. Mirrors are never a source of trust.",
    },
    {
      icon: <Folder24Regular />,
      title: "A private game folder",
      body: "Aether installs into its own folder. It never writes into your existing .minecraft or another launcher’s files.",
    },
    {
      icon: <ArrowDownload24Regular />,
      title: "Import without re-downloading",
      body: "Bring versions over from PCL2, HMCL, BakaXL, Prism or MultiMC. Files that already match the official hash are copied; everything else is downloaded.",
    },
    {
      icon: <Person24Regular />,
      title: "Offline and Microsoft accounts",
      body: "Offline accounts work today. Microsoft sign-in uses the device code flow and keeps tokens in your operating system’s credential vault.",
    },
    {
      icon: <Gauge24Regular />,
      title: "Memory sized to your PC",
      body: "The memory slider reads your real installed and free memory, snaps to 512 MiB steps, and warns before you over-allocate.",
    },
    {
      icon: <Code24Regular />,
      title: "Open source, Rust and Tauri 2",
      body: "A small native app with a Fluent 2 interface. Apache-2.0 licensed, with the design decisions written down in the repository.",
    },
  ],
  statusTitle: "Status",
  statusLead: `Honest and current as of ${lastUpdated}. This site changes when the launcher does.`,
  steps: [
    { tone: "done", label: "Available", name: "Install vanilla versions", detail: "Official release and snapshot versions, with progress and cancel." },
    { tone: "done", label: "Available", name: "Import from other launchers", detail: "PCL2, HMCL, BakaXL, Prism and MultiMC (vanilla versions)." },
    { tone: "done", label: "Available", name: "Offline accounts and launching", detail: "Start an installed instance with the memory you choose." },
    { tone: "wip", label: "In progress", name: "Microsoft sign-in", detail: "Built and tested. Waiting for Mojang to approve the application before it can be switched on." },
    { tone: "todo", label: "Planned", name: "Mod loaders and Java download", detail: "Fabric, Forge, Quilt and NeoForge, plus automatic Java runtimes." },
  ],
};

const zh: Copy = {
  status: "早期开发中：暂无公开发行版。",
  lead: "一个透明、可靠的 Minecraft Java 版启动器，使用 Rust、Tauri 2 和 Fluent 2 构建。",
  primary: "在 GitHub 查看",
  secondary: "阅读安全模型",
  featuresTitle: "它做什么",
  featuresLead: "Aether 围绕几条可以在源码里核对的承诺构建。",
  features: [
    {
      icon: <ShieldCheckmark24Regular />,
      title: "下载都经过校验",
      body: "每个游戏文件在使用前都对照 Mojang 官方清单里的 SHA-1 校验。镜像从来不是信任来源。",
    },
    {
      icon: <Folder24Regular />,
      title: "独立的游戏目录",
      body: "Aether 装在自己的文件夹里，不会写进你现有的 .minecraft 或其他启动器的文件。",
    },
    {
      icon: <ArrowDownload24Regular />,
      title: "导入，不用重新下载",
      body: "从 PCL2、HMCL、BakaXL、Prism、MultiMC 带过来。哈希和官方一致的文件直接复制，其余照常下载。",
    },
    {
      icon: <Person24Regular />,
      title: "离线与微软账户",
      body: "离线账户现在就能用。微软登录用设备码流程，令牌只保存在系统的凭据库里。",
    },
    {
      icon: <Gauge24Regular />,
      title: "按你的电脑配内存",
      body: "内存滑块读取真实的总内存和可用内存，按 512 MiB 吸附，分配过多会提前提醒。",
    },
    {
      icon: <Code24Regular />,
      title: "开源，Rust 与 Tauri 2",
      body: "小巧的原生应用，Fluent 2 界面。Apache-2.0 许可，设计决定都写在仓库里。",
    },
  ],
  statusTitle: "进展",
  statusLead: `截至 ${lastUpdated} 的真实情况。启动器有变化，这个网站就会更新。`,
  steps: [
    { tone: "done", label: "已可用", name: "安装原版", detail: "官方正式版和快照，带进度和取消。" },
    { tone: "done", label: "已可用", name: "从其他启动器导入", detail: "PCL2、HMCL、BakaXL、Prism、MultiMC（原版版本）。" },
    { tone: "done", label: "已可用", name: "离线账户与启动", detail: "用你设定的内存启动已安装的实例。" },
    { tone: "wip", label: "进行中", name: "微软登录", detail: "已完成并测试，等待 Mojang 批准应用之后才能开启。" },
    { tone: "todo", label: "计划中", name: "模组加载器与 Java 下载", detail: "Fabric、Forge、Quilt、NeoForge，以及自动获取 Java。" },
  ],
};

/** Two chromatic badges at most: available (success) and in progress (brand). */
const badges = {
  done: { appearance: "tint", color: "success" },
  wip: { appearance: "tint", color: "brand" },
  todo: { appearance: "outline", color: "subtle" },
} as const;

export function Home({ lang }: { lang: Lang }) {
  const styles = useStyles();
  const copy = lang === "en" ? en : zh;

  return (
    <div className={styles.container}>
      <section className={styles.hero}>
        <img className={styles.heroLogo} src="/logo.svg" alt="" width={56} height={56} />
        <Text as="h1" size={textSize.title} weight="semibold">
          Aether Launcher
        </Text>
        <Text size={textSize.lead} className={styles.lead}>
          {copy.lead}
        </Text>
        <div className={styles.actions}>
          <Button as="a" appearance="primary" href={repositoryUrl} rel="noopener">
            {copy.primary}
          </Button>
          <ButtonLink route={{ lang, page: "security" }}>{copy.secondary}</ButtonLink>
        </div>
        <Text size={textSize.ui} className={styles.caption}>
          {copy.status}
        </Text>
      </section>

      <section className={styles.section} aria-labelledby="features">
        <Text as="h2" id="features" size={textSize.section} weight="semibold">
          {copy.featuresTitle}
        </Text>
        <Text size={textSize.body} className={styles.sectionLead}>
          {copy.featuresLead}
        </Text>
        <div className={styles.grid}>
          {copy.features.map((feature) => (
            <Card key={feature.title} appearance="outline" className={styles.card}>
              <span className={styles.icon} aria-hidden="true">
                {feature.icon}
              </span>
              <Text as="h3" size={textSize.item} weight="semibold">
                {feature.title}
              </Text>
              <Text size={textSize.ui} className={styles.cardBody}>
                {feature.body}
              </Text>
            </Card>
          ))}
        </div>
      </section>

      <section className={styles.section} aria-labelledby="status">
        <Text as="h2" id="status" size={textSize.section} weight="semibold">
          {copy.statusTitle}
        </Text>
        <Text size={textSize.body} className={styles.sectionLead}>
          {copy.statusLead}
        </Text>
        <ul className={styles.steps}>
          {copy.steps.map((step) => (
            <li key={step.name} className={styles.step}>
              <span>
                <Badge {...badges[step.tone]}>{step.label}</Badge>
              </span>
              <span className={styles.stepText}>
                <Text size={textSize.ui} weight="semibold">
                  {step.name}
                </Text>
                <Text size={textSize.ui} className={styles.stepDetail}>
                  {step.detail}
                </Text>
              </span>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
