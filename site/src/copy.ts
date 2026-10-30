import type { Lang, PageId } from "./router";

/** Text shared by every page. Page bodies live next to their pages. */
export type Chrome = {
  htmlLang: string;
  skip: string;
  nav: { home: string; security: string; privacy: string; terms: string; github: string };
  switchTo: string;
  footerDisclaimer: string;
  /** The sentence around the GitHub link: text before it and after it. */
  footerLicense: { before: string; after: string };
  menuLabel: string;
};

export const chrome: Record<Lang, Chrome> = {
  en: {
    htmlLang: "en",
    skip: "Skip to content",
    nav: { home: "Home", security: "Security", privacy: "Privacy", terms: "Terms", github: "GitHub" },
    switchTo: "中文",
    menuLabel: "Main",
    footerDisclaimer:
      "Aether Launcher is an unofficial, open-source project. It is not approved by or associated with Mojang Studios or Microsoft. Minecraft is a trademark of Mojang Studios; Xbox and Microsoft are trademarks of Microsoft.",
    footerLicense: { before: "Licensed under Apache-2.0. Source, issues and releases live on ", after: "." },
  },
  zh: {
    htmlLang: "zh-CN",
    skip: "跳到正文",
    nav: { home: "首页", security: "安全", privacy: "隐私", terms: "条款", github: "GitHub" },
    switchTo: "English",
    menuLabel: "主导航",
    footerDisclaimer:
      "Aether Launcher 是一个非官方的开源项目，未获得 Mojang Studios 或微软的认可，也与它们没有关联。Minecraft 是 Mojang Studios 的商标；Xbox 和 Microsoft 是微软的商标。",
    footerLicense: { before: "以 Apache-2.0 许可证开源。源码、问题反馈和发行版都在 ", after: "。" },
  },
};

export type PageMeta = { title: string; description: string };

export const meta: Record<Lang, Record<PageId, PageMeta>> = {
  en: {
    home: {
      title: "Aether Launcher: a transparent Minecraft launcher",
      description:
        "A transparent, resilient Minecraft: Java Edition launcher built with Rust, Tauri 2 and Fluent 2.",
    },
    security: {
      title: "Security model | Aether Launcher",
      description:
        "The rules Aether Launcher is built around: narrow IPC, no credentials in logs or the UI, verified downloads, and untrusted input handling.",
    },
    privacy: {
      title: "Privacy | Aether Launcher",
      description: "What Aether Launcher and this website do and do not do with your data.",
    },
    terms: {
      title: "Terms of use | Aether Launcher",
      description: "The terms for using Aether Launcher and this website.",
    },
  },
  zh: {
    home: {
      title: "Aether Launcher：透明的 Minecraft 启动器",
      description: "一个透明、可靠的 Minecraft Java 版启动器，使用 Rust、Tauri 2 和 Fluent 2 构建。",
    },
    security: {
      title: "安全模型 | Aether Launcher",
      description:
        "Aether Launcher 遵循的规则：收窄的 IPC、凭据不进日志和界面、下载按官方清单校验、把不可信输入当作不可信。",
    },
    privacy: {
      title: "隐私 | Aether Launcher",
      description: "Aether Launcher 和这个网站会做什么、不会做什么。",
    },
    terms: {
      title: "使用条款 | Aether Launcher",
      description: "使用 Aether Launcher 和这个网站的条款。",
    },
  },
};
