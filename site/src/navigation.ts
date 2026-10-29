import { useSyncExternalStore } from "react";
import { parseRoute } from "./router";
import type { Route } from "./router";

const listeners = new Set<() => void>();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  window.addEventListener("popstate", listener);
  return () => {
    listeners.delete(listener);
    window.removeEventListener("popstate", listener);
  };
}

function pathname(): string {
  return window.location.pathname;
}

/** Moves to `href` without reloading, like a normal link click would. */
export function navigate(href: string): void {
  if (href === window.location.pathname) {
    return;
  }
  window.history.pushState(null, "", href);
  window.scrollTo(0, 0);
  listeners.forEach((listener) => listener());
}

/** The route for the current address; re-renders when it changes. */
export function useRoute(): Route {
  return parseRoute(useSyncExternalStore(subscribe, pathname));
}
