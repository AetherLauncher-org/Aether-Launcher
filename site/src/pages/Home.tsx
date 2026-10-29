import { Badge, Card, Text, makeStyles, mergeClasses, shorthands, tokens } from "@fluentui/react-components";
import {
  ArrowDownload24Regular,
  Code24Regular,
  Folder24Regular,
  Gauge24Regular,
  Person24Regular,
  ShieldCheckmark24Regular,
} from "@fluentui/react-icons";
import type { ReactNode } from "react";
import { Button } from "@fluentui/react-components";
import { ButtonLink } from "../chrome";
import { lastUpdated, repositoryUrl } from "../router";
import type { Lang } from "../router";

const useStyles = makeStyles({
  hero: {
    position: "relative",
    overflow: "hidden",
    paddingBlock: "72px 56px",
    backgroundImage:
      "radial-gradient(60% 80% at 15% 0%, rgb(47 192 220 / 16%), transparent 70%), radial-gradient(60% 80% at 90% 10%, rgb(123 63 255 / 14%), transparent 70%)",
    "@media (max-width: 720px)": { paddingBlock: "32px 24px" },
  },
  heroInner: {
    display: "grid",
    gridTemplateColumns: "1fr auto",
    alignItems: "center",
    columnGap: "40px",
    maxWidth: "1080px",
    marginInline: "auto",
    paddingInline: "16px",
    "@media (max-width: 720px)": { gridTemplateColumns: "1fr", rowGap: "16px" },
  },
  heroLogo: {
    width: "200px",
    height: "200px",
    filter: "drop-shadow(0 16px 32px rgb(59 120 255 / 30%))",
    animationName: {
      "50%": { transform: "translateY(-8px)" },
    },
    animationDuration: "6s",
    animationIterationCount: "infinite",
    animationTimingFunction: "ease-in-out",
    "@media (max-width: 720px)": { order: -1, width: "96px", height: "96px" },
    "@media (prefers-reduced-motion: reduce)": { animationName: "none" },
  },
  chip: { marginBottom: "16px" },
  title: {
    display: "block",
    marginBottom: "12px",
    fontFamily: tokens.fontFamilyBase,
  },
  lead: {
    display: "block",
    maxWidth: "56ch",
    marginBottom: "24px",
    color: tokens.colorNeutralForeground2,
  },
  actions: { display: "flex", flexWrap: "wrap", columnGap: "12px", rowGap: "12px" },
  body: {
    maxWidth: "1080px",
    marginInline: "auto",
    paddingInline: "16px",
  },
  section: { paddingBlock: "40px 8px" },
  sectionLead: {
    display: "block",
    maxWidth: "70ch",
    marginBlock: "8px 24px",
    color: tokens.colorNeutralForeground2,
  },
  grid: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))",
    columnGap: "16px",
    rowGap: "16px",
  },
  card: {
    rowGap: "8px",
    animationName: { from: { opacity: 0, transform: "translateY(8px)" } },
    animationDuration: tokens.durationNormal,
    animationTimingFunction: tokens.curveDecelerateMid,
    animationFillMode: "both",
    transitionProperty: "transform, box-shadow",
    transitionDuration: tokens.durationNormal,
    transitionTimingFunction: tokens.curveDecelerateMid,
    ":hover": { transform: "translateY(-2px)", boxShadow: tokens.shadow8 },
    "@media (prefers-reduced-motion: reduce)": { animationName: "none", transitionDuration: "0ms" },
  },
  iconBubble: {
    display: "grid",
    placeItems: "center",
    width: "36px",
    height: "36px",
    borderRadius: tokens.borderRadiusMedium,
    backgroundColor: tokens.colorBrandBackground2,
    color: tokens.colorBrandForeground1,
  },
  status: { display: "grid", rowGap: "8px", margin: 0, padding: 0, listStyleType: "none" },
  statusRow: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    columnGap: "12px",
    rowGap: "4px",
    paddingBlock: "12px",
    paddingInline: "16px",
    borderRadius: tokens.borderRadiusMedium,
    backgroundColor: tokens.colorNeutralBackground1,
    ...shorthands.border("1px", "solid", tokens.colorNeutralStroke2),
  },
  statusBadge: { minWidth: "84px", justifyContent: "center" },
  statusText: { display: "grid", flexGrow: 1, minWidth: "220px" },
});

type Feature = { icon: ReactNode; title: string; body: string };
type Step = { tone: "done" | "wip" | "todo"; label: string; name: string; detail: string };

type Copy = {
  chip: string;
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
  chip: "Early development: no public release yet",
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
  chip: "早期开发中：暂无公开发行版",
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

const tones = { done: "success", wip: "warning", todo: "informative" } as const;

export function Home({ lang }: { lang: Lang }) {
  const styles = useStyles();
  const copy = lang === "en" ? en : zh;

  return (
    <>
      <section className={styles.hero}>
        <div className={styles.heroInner}>
          <div>
            <Badge className={styles.chip} appearance="tint" color="warning" size="large">
              {copy.chip}
            </Badge>
            <Text as="h1" size={1000} weight="semibold" className={styles.title}>
              Aether Launcher
            </Text>
            <Text size={500} className={styles.lead}>
              {copy.lead}
            </Text>
            <div className={styles.actions}>
              <Button as="a" appearance="primary" size="large" href={repositoryUrl} rel="noopener">
                {copy.primary}
              </Button>
              <ButtonLink route={{ lang, page: "security" }} size="large">
                {copy.secondary}
              </ButtonLink>
            </div>
          </div>
          <img className={styles.heroLogo} src="/logo.svg" alt="" width={200} height={200} />
        </div>
      </section>

      <div className={styles.body}>
        <section className={styles.section} aria-labelledby="features">
          <Text as="h2" id="features" size={800} weight="semibold">
            {copy.featuresTitle}
          </Text>
          <Text size={400} className={styles.sectionLead}>
            {copy.featuresLead}
          </Text>
          <div className={styles.grid}>
            {copy.features.map((feature, index) => (
              <Card key={feature.title} className={styles.card} style={{ animationDelay: `${index * 60}ms` }}>
                <span className={styles.iconBubble} aria-hidden="true">
                  {feature.icon}
                </span>
                <Text as="h3" size={400} weight="semibold">
                  {feature.title}
                </Text>
                <Text size={300}>{feature.body}</Text>
              </Card>
            ))}
          </div>
        </section>

        <section className={styles.section} aria-labelledby="status">
          <Text as="h2" id="status" size={800} weight="semibold">
            {copy.statusTitle}
          </Text>
          <Text size={400} className={styles.sectionLead}>
            {copy.statusLead}
          </Text>
          <ul className={styles.status}>
            {copy.steps.map((step) => (
              <li key={step.name} className={styles.statusRow}>
                <Badge
                  className={mergeClasses(styles.statusBadge)}
                  appearance="tint"
                  color={tones[step.tone]}
                >
                  {step.label}
                </Badge>
                <span className={styles.statusText}>
                  <Text weight="semibold">{step.name}</Text>
                  <Text size={200} style={{ color: tokens.colorNeutralForeground3 }}>
                    {step.detail}
                  </Text>
                </span>
              </li>
            ))}
          </ul>
        </section>
      </div>
    </>
  );
}
