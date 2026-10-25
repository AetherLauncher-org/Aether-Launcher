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
import { useState } from "react";
import type { FormEvent } from "react";
import { createInstance } from "../ipc";
import type { InstanceSummary } from "../ipc";
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
  };

  const close = () => {
    reset();
    onOpenChange(false);
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
                  }}
                >
                  {suggestedVersions.map((item) => (
                    <option value={item} key={item}>
                      {item}
                    </option>
                  ))}
                  <option value={customVersionValue}>手动填写…</option>
                </Select>
              </Field>

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
