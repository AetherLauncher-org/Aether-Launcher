/**
 * 页面文案的键映射：`navigation.tsx` 的 `PageDefinition` 里仍是中文原文，
 * 在消费点用这里按 `page.id` 取键，避免把 i18n 键塞进共享的页面定义。
 */
import type { PageId } from "../navigation";
import type { MessageKey } from "./catalogs";

export const pageLabelKeys: Readonly<Record<PageId, MessageKey>> = {
  home: "page.home.label",
  instances: "page.instances.label",
  download: "page.download.label",
  accounts: "page.accounts.label",
  settings: "page.settings.label",
};
