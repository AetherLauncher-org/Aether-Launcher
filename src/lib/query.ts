import { useCallback, useEffect, useRef, useState } from "react";

/**
 * 一次异步读取的状态机。
 *
 * 三个页面（实例、资源库、设置里的 Java 列表）都是同一个形状：
 * 挂载时读一次、可以手动重试、卸载后不再写 state。这里把它收成一个小 hook，
 * 页面只关心「拿到数据以后怎么排」，不重复写 loading / error / retry 的样板。
 */
export type QueryStatus = "idle" | "loading" | "success" | "error";

export type QueryState<T> = {
  status: QueryStatus;
  data: T | null;
  error: string | null;
};

export type QueryResult<T> = QueryState<T> & {
  /** 重新读取。会先把状态切回 loading。 */
  reload: () => void;
};

const idleState: QueryState<never> = {
  status: "idle",
  data: null,
  error: null,
};

/**
 * `load` 需要是稳定引用（`useCallback` 或者模块级函数），否则每次渲染都会重新读取。
 * 传 `enabled: false` 时完全不发请求，用于「非桌面版」这类确定拿不到数据的情况。
 */
export function useQuery<T>(
  load: () => Promise<T>,
  enabled = true,
): QueryResult<T> {
  const [state, setState] = useState<QueryState<T>>(
    enabled ? { ...idleState, status: "loading" } : idleState,
  );
  const [attempt, setAttempt] = useState(0);

  // 只接受最近一次请求的结果，避免快速点两次刷新时旧结果覆盖新结果
  const latest = useRef(0);

  useEffect(() => {
    // 关闭时不发请求。状态本来就没离开过 idle，不需要在 effect 里再写一次
    if (!enabled) {
      return;
    }

    const ticket = (latest.current += 1);
    let cancelled = false;

    // 先切到 loading，再等结果。用 microtask 推出去，规则不允许在
    // effect 体里同步 setState（会触发额外的渲染）
    void Promise.resolve().then(() => {
      if (!cancelled && ticket === latest.current) {
        setState((previous) => ({
          status: "loading",
          data: previous.data,
          error: null,
        }));
      }
    });

    load()
      .then((data) => {
        if (!cancelled && ticket === latest.current) {
          setState({ status: "success", data, error: null });
        }
      })
      .catch((error: unknown) => {
        if (cancelled || ticket !== latest.current) {
          return;
        }
        setState({
          status: "error",
          data: null,
          error: describeError(error),
        });
      });

    return () => {
      cancelled = true;
    };
  }, [load, enabled, attempt]);

  const reload = useCallback(() => {
    setAttempt((value) => value + 1);
  }, []);

  return { ...state, reload };
}

function describeError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  return "读取失败";
}
