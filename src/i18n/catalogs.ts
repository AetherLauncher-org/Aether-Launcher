/**
 * 打包进来的语言目录。`zh-CN` 是源语言（source of truth），也是回退目标。
 *
 * 用静态 JSON import：`tsconfig` 开了 `resolveJsonModule`，Vite 原生支持 JSON
 * import，所以目录会随主包一起打包、编译期内联，没有任何运行时网络请求。
 */
import zhCN from "../../locales/zh-CN.json";
import enUS from "../../locales/en-US.json";
import type { MessageCatalog } from "./messages";

/** 已登记的语言。新增语言时在这里加一行，并在 `locales/` 放对应文件。 */
export const bundledCatalogs = {
  "zh-CN": zhCN as MessageCatalog,
  "en-US": enUS as MessageCatalog,
} as const satisfies Readonly<Record<string, MessageCatalog>>;

export type Locale = keyof typeof bundledCatalogs;

/** 源语言：缺翻译时的回退目标。 */
export const defaultLocale: Locale = "zh-CN";

/** 所有可用语言，按展示顺序。 */
export const locales: readonly Locale[] = ["zh-CN", "en-US"];

/** 语言在本语言里的显示名（下拉框里用，不做翻译，各语言用自称）。 */
export const localeDisplayNames: Readonly<Record<Locale, string>> = {
  "zh-CN": "简体中文",
  "en-US": "English",
};

/** 源语言目录里出现的键：`t()` 的键类型约束，打错键名会编译失败。 */
export type MessageKey = keyof typeof zhCN & string;
