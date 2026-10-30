import { Badge, Button, Text } from "@fluentui/react-components";
import { Add24Regular, ArrowSync24Regular } from "@fluentui/react-icons";
import { useCallback, useEffect, useMemo, useState } from "react";
import { discoverJavaRuntimes, isTauri, pickJavaRuntime } from "../ipc";
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
 * 列表来自后端的缓存：平时只读 `java-runtimes.json`，点「重新扫描」才会真的
 * 去扫。扫描要启动一串 `java -version`，每次切到主页/设置都重扫会让界面反复
 * 卡顿，以前还会在任务栏闪出命令行窗口。
 *
 * 「导入 Java…」打开系统的文件选择框，让用户指一个扫描器找不到的安装
 * （IDE 自带的 JBR、解压到任意目录的 JDK 之类）。后端会探测并记进缓存。
 */
export function JavaRuntimeSection() {
  const desktop = isTauri();
  const [runtimes, setRuntimes] = useState<JavaRuntimeInfo[]>([]);
  const [loading, setLoading] = useState(desktop);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedKey, setSavedKey] = useState<string | null>(loadJavaRuntimeKey);
  // 递增这个令牌表示「再扫一次」，effect 只依赖它，不在 effect 里同步写 state
  const [scanToken, setScanToken] = useState(0);

  // 扫描是外部读取，结果回来才写 state；effect 体里只发起请求。
  // 第一次（token 为 0）读缓存，之后每次点按钮都要求真的重扫。
  useEffect(() => {
    if (!desktop) {
      return;
    }

    let cancelled = false;
    void discoverJavaRuntimes(scanToken > 0).then((found) => {
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

  const rescan = useCallback(() => {
    setError(null);
    setLoading(true);
    setScanToken((value) => value + 1);
  }, []);

  const importJava = useCallback(async () => {
    if (!desktop || importing) {
      return;
    }
    setError(null);
    setImporting(true);
    try {
      const picked = await pickJavaRuntime();
      // null = 用户取消了文件框，什么都不用做。
      if (picked === null) {
        return;
      }
      setRuntimes((current) => sortRuntimes(dedupeRuntimes([...current, picked])));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setImporting(false);
    }
  }, [desktop, importing]);

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
        <div className="runtime-actions">
          <Button
            appearance="subtle"
            icon={<Add24Regular />}
            onClick={() => void importJava()}
            disabled={!desktop || importing}
          >
            导入 Java…
          </Button>
          <Button
            appearance="subtle"
            icon={<ArrowSync24Regular />}
            onClick={rescan}
            disabled={!desktop || loading}
          >
            重新扫描
          </Button>
        </div>
      </div>
      <Text className="muted">
        本机检测到的 Java。列表保存在本机，只有点「重新扫描」才会重新检测。
        在盘符根目录下的文件夹里翻到的 Java 只读取旁边的 release 文件来显示版本，
        不会被运行，启动游戏也不会用它；想用的话请点「导入 Java…」亲自选一次。
        启动时会按游戏版本的要求自动挑选合适的一个；这里选中的默认运行时暂不影响启动。
      </Text>

      {error !== null ? (
        <Text className="runtime-status runtime-error" role="alert">
          {error}
        </Text>
      ) : null}

      {loading ? (
        <Text className="muted runtime-status">正在读取 Java 列表…</Text>
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
