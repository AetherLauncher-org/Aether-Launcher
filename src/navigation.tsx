import {
  ArrowDownload24Filled,
  ArrowDownload24Regular,
  Box24Filled,
  Box24Regular,
  Home24Filled,
  Home24Regular,
  Person24Filled,
  Person24Regular,
  Settings24Filled,
  Settings24Regular,
} from "@fluentui/react-icons";
import type { ReactElement } from "react";

export type PageId = "home" | "instances" | "download" | "accounts" | "settings";

export type PageDefinition = {
  id: PageId;
  label: string;
  /** 页头标题下方的一句话说明 */
  summary: string;
  icon: ReactElement;
  /** 选中时用实心图标，Fluent 的导航惯例 */
  activeIcon: ReactElement;
};

export const mainPages: readonly PageDefinition[] = [
  {
    id: "home",
    label: "主页",
    summary: "继续上次的游戏，或开始新的冒险",
    icon: <Home24Regular />,
    activeIcon: <Home24Filled />,
  },
  {
    id: "instances",
    label: "实例",
    summary: "每个实例的版本与存档相互独立",
    icon: <Box24Regular />,
    activeIcon: <Box24Filled />,
  },
  {
    id: "download",
    label: "下载",
    summary: "从 Mojang 官方安装游戏版本，每个文件都会校验",
    icon: <ArrowDownload24Regular />,
    activeIcon: <ArrowDownload24Filled />,
  },
];

/**
 * 账户页不在主页面那一排里：进去的入口是导航栏右边的当前账户（头像和名字），见 `NavPane`。
 * 页面本身和别的页一样，有标题和说明。
 */
export const accountsPage: PageDefinition = {
  id: "accounts",
  label: "账户",
  summary: "用哪个身份进入游戏",
  icon: <Person24Regular />,
  activeIcon: <Person24Filled />,
};

export const settingsPage: PageDefinition = {
  id: "settings",
  label: "设置",
  summary: "外观、内存、Java 与下载来源",
  icon: <Settings24Regular />,
  activeIcon: <Settings24Filled />,
};

export const allPages: readonly PageDefinition[] = [...mainPages, accountsPage, settingsPage];

export function findPage(id: PageId): PageDefinition {
  return allPages.find((page) => page.id === id) ?? allPages[0];
}
