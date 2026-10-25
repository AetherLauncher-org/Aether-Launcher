import {
  Button,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Field,
  Input,
  Select,
  Text,
} from "@fluentui/react-components";
import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import {
  cacheVanillaVersionManifest,
  installVanillaClientLibraries,
  createInstance,
  describeCommandError,
  isTauri,
  listMinecraftVersions,
} from "../ipc";
import type { InstanceSummary, MinecraftVersionCatalog } from "../ipc";
import {
  suggestedVersions,
  validateInstanceName,
  validateInstanceVersion,
} from "../lib/instances";
import {
  clampMemoryMiB,
  defaultMemoryMiB,
  formatMemoryMiB,
  maxMemoryMiB,
  minMemoryMiB,
} from "../lib/memory";

type CreateInstanceDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 新建成功后回调，页面据此刷新列表 */
  onCreated: (instance: InstanceSummary) => void;
};

const customVersionValue = "__custom__";

/**
 * 创建实例对话框：名称、版本、内存。
 *
 * 内存目前只写在 `localStorage`（`aether.memoryMiB`）里，作为实例页展示和
 * 后续启动的默认上限——`create_instance` 命令只接收名称与版本，没有内存字段。
 */
export function CreateInstanceDialog({
  open,
  onOpenChange,
  onCreated,
}: CreateInstanceDialogProps) {
  const [name, setName] = useState("");
  const [version, setVersion] = useState(suggestedVersions[0]);
  const [customVersion, setCustomVersion] = useState("");
  const [memory, setMemory] = useState(String(defaultMemoryMiB));
  const [nameError, setNameError] = useState<string | null>(null);
  const [versionError, setVersionError] = useState<string | null>(null);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [caching, setCaching] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [cacheResult, setCacheResult] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<MinecraftVersionCatalog | null>(null);
  const [catalogError, setCatalogError] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !isTauri()) {
      return;
    }
    let active = true;
    void listMinecraftVersions()
      .then((result) => {
        if (active) {
          setCatalog(result);
          setVersion(result.latest.release);
        }
      })
      .catch((error: unknown) => {
        if (active) {
          setCatalogError(describeCommandError(error));
        }
      })
    return () => {
      active = false;
    };
  }, [open]);

  const releases = catalog?.versions
    .filter((item) => item.type === "release")
    .slice(0, 90) ?? [];
  const snapshots = catalog?.versions
    .filter((item) => item.type === "snapshot")
    .slice(0, 20) ?? [];

  const catalogLoading = open && isTauri() && !catalog && !catalogError;

  const usingCustomVersion = version === customVersionValue;
  const effectiveVersion = usingCustomVersion ? customVersion : version;

  const reset = () => {
    setName("");
    setVersion(suggestedVersions[0]);
    setCustomVersion("");
    setMemory(String(defaultMemoryMiB));
    setNameError(null);
    setVersionError(null);
    setSubmitError(null);
    setSubmitting(false);
    setCaching(false);
    setInstalling(false);
    setCacheResult(null);
    setCatalog(null);
    setCatalogError(null);
  };

  const close = () => {
    reset();
    onOpenChange(false);
  };

  const handleCache = async () => {
    const error = validateInstanceVersion(effectiveVersion);
    if (error !== null) {
      setVersionError(error);
      return;
    }
    setCaching(true);
    setCacheResult(null);
    try {
      const cached = await cacheVanillaVersionManifest(effectiveVersion.trim());
      setCacheResult(`已校验并缓存 ${cached} 的官方版本 JSON。客户端及资源尚未安装。`);
    } catch (error: unknown) {
      setCacheResult(`缓存失败：${describeCommandError(error)}`);
    } finally {
      setCaching(false);
    }
  };

  const handleInstall = async () => {
    const error = validateInstanceVersion(effectiveVersion);
    if (error !== null) {
      setVersionError(error);
      return;
    }
    setInstalling(true);
    setCacheResult(null);
    try {
      const id = effectiveVersion.trim();
      await cacheVanillaVersionManifest(id);
      const count = await installVanillaClientLibraries(id);
      setCacheResult(`已下载并校验 ${count} 个文件（客户端、依赖库、资源）。现在可以创建实例并启动。`);
    } catch (error: unknown) {
      setCacheResult(`安装失败：${describeCommandError(error)}`);
    } finally {
      setInstalling(false);
    }
  };

  const handleSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setSubmitError(null);

    const nameMessage = validateInstanceName(name);
    const versionMessage = validateInstanceVersion(effectiveVersion);
    setNameError(nameMessage);
    setVersionError(versionMessage);
    if (nameMessage !== null || versionMessage !== null) {
      return;
    }

    setSubmitting(true);
    try {
      const created = await createInstance({
        name,
        version: effectiveVersion,
      });
      // 内存是本机默认值，存在 localStorage，不走 IPC
      try {
        window.localStorage.setItem(
          "aether.memoryMiB",
          String(clampMemoryMiB(Number.parseInt(memory, 10))),
        );
      } catch {
        // 存储不可用时忽略，创建本身已经成功
      }
      onCreated(created);
      close();
    } catch (error: unknown) {
      setSubmitError(
        typeof error === "string"
          ? error
          : error instanceof Error
            ? error.message
            : "创建失败",
      );
      setSubmitting(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(_, data) => {
        if (!data.open) {
          close();
        }
      }}
    >
      <DialogSurface aria-describedby={undefined}>
        <form onSubmit={(event) => void handleSubmit(event)}>
          <DialogBody>
            <DialogTitle>创建实例</DialogTitle>
            <DialogContent className="dialog-content">
              <Field
                label="实例名称"
                required
                validationMessage={nameError ?? undefined}
                validationState={nameError ? "error" : "none"}
              >
                <Input
                  value={name}
                  autoFocus
                  placeholder="例如 生存 1.21"
                  onChange={(_, data) => {
                    setName(data.value);
                    if (nameError) {
                      setNameError(null);
                    }
                  }}
                />
              </Field>

              <Field label="游戏版本" required>
                <Select
                  value={version}
                  onChange={(_, data) => {
                    setVersion(data.value);
                    setVersionError(null);
                    setCacheResult(null);
                  }}
                >
                  {catalog ? (
                    <>
                      <optgroup label="正式版">
                        {releases.map((item) => (
                          <option value={item.id} key={item.id}>
                            {item.id}
                          </option>
                        ))}
                      </optgroup>
                      <optgroup label="快照版">
                        {snapshots.map((item) => (
                          <option value={item.id} key={item.id}>
                            {item.id}
                          </option>
                        ))}
                      </optgroup>
                    </>
                  ) : (
                    suggestedVersions.map((item) => (
                      <option value={item} key={item}>
                        {item}
                      </option>
                    ))
                  )}
                  <option value={customVersionValue}>手动填写…</option>
                </Select>
              </Field>

              <Text size={200}>创建只保存实例记录；请先下载并安装所选版本，再创建并启动。</Text>
              {isTauri() ? (
                <Button
                  type="button"
                  appearance="secondary"
                  disabled={caching || installing || submitting || effectiveVersion.trim().length === 0}
                  onClick={() => void handleCache()}
                >
                  {caching ? "校验并缓存中…" : "缓存此版本的官方 JSON"}
                </Button>
              ) : null}
              {isTauri() ? (
                <Button
                  type="button"
                  appearance="secondary"
                  disabled={caching || installing || submitting || effectiveVersion.trim().length === 0}
                  onClick={() => void handleInstall()}
                >
                  {installing ? "下载并校验中，可能需要几分钟…" : "下载并安装此版本（客户端、依赖库、资源）"}
                </Button>
              ) : null}
              {cacheResult ? <Text size={200} role="status">{cacheResult}</Text> : null}

              {catalogLoading ? (
                <Text size={200}>正在读取 Mojang 官方版本目录…</Text>
              ) : catalogError ? (
                <Text size={200} role="status">
                  官方版本目录暂不可用：{catalogError}。仍可手动填写版本号。
                </Text>
              ) : catalog ? (
                <Text size={200}>已从 Mojang 读取版本目录；创建实例暂不下载游戏文件。</Text>
              ) : null}

              {usingCustomVersion ? (
                <Field
                  label="版本号"
                  required
                  validationMessage={versionError ?? undefined}
                  validationState={versionError ? "error" : "none"}
                >
                  <Input
                    value={customVersion}
                    placeholder="例如 1.21.1"
                    onChange={(_, data) => {
                      setCustomVersion(data.value);
                      setCacheResult(null);
                      if (versionError) {
                        setVersionError(null);
                      }
                    }}
                  />
                </Field>
              ) : null}

              <Field
                label="内存上限"
                hint={`${formatMemoryMiB(minMemoryMiB)} – ${formatMemoryMiB(
                  maxMemoryMiB,
                )}`}
              >
                <Input
                  type="number"
                  value={memory}
                  min={minMemoryMiB}
                  max={maxMemoryMiB}
                  step={256}
                  contentBefore={<Text size={200}>MiB</Text>}
                  onChange={(_, data) => {
                    setMemory(data.value);
                  }}
                  onBlur={() => {
                    setMemory(
                      String(clampMemoryMiB(Number.parseInt(memory, 10))),
                    );
                  }}
                />
              </Field>

              {submitError ? (
                <Text className="field-error" role="alert">
                  {submitError}
                </Text>
              ) : null}
            </DialogContent>

            <DialogActions>
              <Button appearance="secondary" type="button" onClick={close}>
                取消
              </Button>
              <Button
                appearance="primary"
                type="submit"
                disabled={submitting}
              >
                {submitting ? "创建中…" : "创建"}
              </Button>
            </DialogActions>
          </DialogBody>
        </form>
      </DialogSurface>
    </Dialog>
  );
}
