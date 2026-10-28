import { createContext, useContext } from "react";
import type { PageId } from "../navigation";

/** 页面之间的跳转（例如主页的「去下载」）。页面本身不持有路由。 */
export type NavigationApi = {
  current: PageId;
  navigate: (page: PageId) => void;
};

export const NavigationContext = createContext<NavigationApi | null>(null);

export function useNavigation(): NavigationApi {
  const value = useContext(NavigationContext);
  if (value === null) {
    throw new Error("useNavigation 需要在 NavigationContext 内使用");
  }
  return value;
}
