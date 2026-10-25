import {
  Box24Regular,
  Home24Filled,
  Library24Regular,
  Person24Regular,
  Settings24Regular,
} from "@fluentui/react-icons";
import type { ReactElement } from "react";

export type PageId = "home" | "instances" | "library" | "accounts" | "settings";

export type PageDefinition = {
  id: PageId;
  label: string;
  icon: ReactElement;
  /** 页头下方的一句话说明 */
  summary: string;
  /** 功能还没做时，页面里显示的标题和说明 */
  pendingTitle: string;
  pendingBody: string;
};

export const mainPages: readonly PageDefinition[] = [
  {
    id: "home",
    label: "主页",
    icon: <Home24Filled />,
    summary: "实例仍属于你，下载链路也应该看得见",
    pendingTitle: "",
    pendingBody: "",
  },
  {
    id: "instances",
    label: "实例",
    icon: <Box24Regular />,
    summary: "每个实例的版本、模组、Java 与启动参数相互隔离",
    // 这三页已经不再是占位：导入等未完成能力由页面自己的提示承担，
    // 这里留空，避免同一句说明在 DOM 里出现两次
    pendingTitle: "",
    pendingBody: "",
  },
  {
    id: "library",
    label: "资源库",
    icon: <Library24Regular />,
    summary: "模组、资源包与它们的下载来源",
    pendingTitle: "",
    pendingBody: "",
  },
  {
    id: "accounts",
    label: "账户",
    icon: <Person24Regular />,
    summary: "微软正版账户与离线账户",
    pendingTitle: "",
    pendingBody: "",
  },
];

export const settingsPage: PageDefinition = {
  id: "settings",
  label: "设置",
  icon: <Settings24Regular />,
  summary: "外观、Java 运行时、内存与下载来源",
  pendingTitle: "Java 与下载来源设置尚未实现",
  pendingBody: "Java 运行时选择、内存上限和下载来源的配置还在开发。",
};

export const allPages: readonly PageDefinition[] = [...mainPages, settingsPage];
