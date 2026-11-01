import type { PageId } from "../navigation";
import type { MessageKey } from "./catalogs";

export const pageLabelKeys: Readonly<Record<PageId, MessageKey>> = {
  home: "page.home.label",
  instances: "page.instances.label",
  download: "page.download.label",
  accounts: "page.accounts.label",
  settings: "page.settings.label",
};
