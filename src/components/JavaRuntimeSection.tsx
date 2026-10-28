import { Badge, Button, Text } from "@fluentui/react-components";
import { ArrowSync24Regular } from "@fluentui/react-icons";
import { useEffect, useMemo, useState } from "react";
import { discoverJavaRuntimes, isTauri } from "../ipc";
import type { JavaRuntimeInfo } from "../ipc";
import {
  dedupeRuntimes,
  describeEmptyRuntimes,
  describeRuntime,
  pickSelectedRuntime,
  runtimeKey,
  shortenPath,
  sortRuntimes,
  vendorLabel,
} from "../lib/javaRuntime";
import { loadJavaRuntimeKey, saveJavaRuntimeKey } from "../lib/settings";

/**
 * 「Java 运行时」设置区块。
 *
 * `list_java_runtimes` 命令可能还不存在（PR #12 尚未合并），所以读取失败
 * 一律降级成空列表 + 一句说明，不留白屏，也不抛错。
 */
export function JavaRuntimeSection() {
  const desktop = isTauri();
  const [runtimes, setRuntimes] = useState<JavaRuntimeInfo[]>([]);
  const [loading, setLoading] = useState(desktop);
  const [savedKey, setSavedKey] = useState<string | null>(loadJavaRuntimeKey);
  // 递增这个令牌表示「再扫一次」，effect 只依赖它，不在 effect 里同步写 state
  const [scanToken, setScanToken] = useState(0);

  // 扫描是外部读取，结果回来才写 state；effect 体里只发起请求
  useEffect(() => {
    if (!desktop) {
      return;
    }

    let cancelled = false;
    void discoverJavaRuntimes().then((found) => {
      if (cancelled) {
        return;
      }
      setRuntimes(sortRuntimes(dedupeRuntimes(found)));
      setLoading(false);
    });

    return () => {
      cancelled = true;
    };
  }, [desktop, scanToken]);

  const rescan = () => {
    setLoading(true);
    setScanToken((value) => value + 1);
  };

  const selected = useMemo(
    () => pickSelectedRuntime(runtimes, savedKey),
    [runtimes, savedKey],
  );

  const choose = (runtime: JavaRuntimeInfo) => {
    const key = runtimeKey(runtime);
    setSavedKey(key);
    saveJavaRuntimeKey(key);
  };

  return (
    <section className="status-panel setting-group" aria-labelledby="java-heading">
      <div className="panel-heading">
        <Text id="java-heading" as="h2" size={400} weight="semibold">
          Java 运行时
        </Text>
        <Button
          appearance="subtle"
          icon={<ArrowSync24Regular />}
          onClick={rescan}
          disabled={!desktop || loading}
        >
          重新扫描
        </Button>
      </div>
      <Text className="muted">
        本机检测到的 Java。启动时会按游戏版本的要求自动挑选合适的一个；
        手动指定默认运行时的功能还在开发，这里的选择暂不影响启动。
      </Text>

      {loading ? (
        <Text className="muted runtime-status">正在扫描本机 Java…</Text>
      ) : runtimes.length === 0 ? (
        <div className="runtime-empty">
          <Badge appearance="outline">暂未检测到 Java</Badge>
          <Text className="muted">{describeEmptyRuntimes(desktop)}</Text>
        </div>
      ) : (
        <ul className="runtime-list" role="radiogroup" aria-label="默认 Java 运行时">
          {runtimes.map((runtime) => {
            const key = runtimeKey(runtime);
            const isSelected = selected !== null && runtimeKey(selected) === key;

            return (
              <li key={key}>
                <button
                  type="button"
                  role="radio"
                  aria-checked={isSelected}
                  className={`runtime-option${isSelected ? " selected" : ""}`}
                  onClick={() => choose(runtime)}
                >
                  <span className="runtime-main">
                    <Text weight="semibold">{describeRuntime(runtime)}</Text>
                    <Text className="muted runtime-path">
                      {shortenPath(runtime.executable)}
                    </Text>
                  </span>
                  <span className="runtime-tags">
                    <Badge appearance="outline">{vendorLabel(runtime)}</Badge>
                    {isSelected ? (
                      <Badge appearance="tint" color="success">
                        默认
                      </Badge>
                    ) : null}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
