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
  MessageBar,
  MessageBarBody,
  Select,
  Spinner,
  Text,
} from "@fluentui/react-components";
import { useState } from "react";
import type { FormEvent } from "react";
import { convertInstanceSchematic, describeCommandError, getConversionTargets } from "../ipc";
import type { ConversionResult, SchematicFormat } from "../ipc";
import { useQuery } from "../lib/query";

type ConvertSchematicDialogProps = {
  open: boolean;
  instanceId: string;
  /** 要转换的文件（纯文件名） */
  fileName: string;
  /** 这个文件现在的格式和数据版本，用来给选项加说明 */
  currentFormat: SchematicFormat | null;
  currentDataVersion: number | null;
  onClose: () => void;
  /** 转换成功，新文件已经在文件夹里 */
  onConverted: (result: ConversionResult) => void;
};

const KEEP = "keep";
const CUSTOM = "custom";

/**
 * 转换投影：换一种格式，也可以同时换 Minecraft 数据版本。结果存成同一文件夹里的新文件，
 * 原文件不动。
 *
 * 换格式是精确的；换数据版本只做确定无疑的几件事（见 `docs/modules/schematics.md`），其余
 * 原样保留，并在转换之后把需要检查的地方列出来。
 */
export function ConvertSchematicDialog({
  open,
  instanceId,
  fileName,
  currentFormat,
  currentDataVersion,
  onClose,
  onConverted,
}: ConvertSchematicDialogProps) {
  const targets = useQuery(getConversionTargets, open);
  const [format, setFormat] = useState<SchematicFormat | null>(null);
  const [version, setVersion] = useState<string>(KEEP);
  const [custom, setCustom] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ConversionResult | null>(null);

  const formats = targets.data?.formats ?? [];
  const chosenFormat = format ?? formats.find((option) => option.label !== currentFormat)?.label ?? formats[0]?.label ?? null;

  const customNumber = Number.parseInt(custom, 10);
  const customValid = version !== CUSTOM || (Number.isInteger(customNumber) && customNumber >= 1 && customNumber <= 1_000_000);
  const dataVersion = version === KEEP ? null : version === CUSTOM ? customNumber : Number.parseInt(version, 10);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (chosenFormat === null || !customValid) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const converted = await convertInstanceSchematic(instanceId, fileName, chosenFormat, dataVersion);
      setResult(converted);
      onConverted(converted);
    } catch (failure: unknown) {
      setError(describeCommandError(failure));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(_, data) => !busy && !data.open && onClose()}>
      <DialogSurface aria-describedby={undefined}>
        <form onSubmit={(event) => void submit(event)}>
          <DialogBody>
            <DialogTitle>转换投影</DialogTitle>
            <DialogContent className="dialog-content">
              <Text>
                「{fileName}」会转换成一个新文件，放在同一个文件夹里，原文件不会被改动。
              </Text>

              {targets.status === "loading" && targets.data === null ? <Spinner size="tiny" label="正在读取可选项…" /> : null}
              {targets.status === "error" && targets.error ? (
                <MessageBar intent="error" role="alert">
                  <MessageBarBody>{targets.error}</MessageBarBody>
                </MessageBar>
              ) : null}

              {result === null ? (
                <>
                  <Field label="目标格式">
                    <Select
                      aria-label="目标格式"
                      value={chosenFormat ?? ""}
                      onChange={(_, data) => setFormat(data.value as SchematicFormat)}
                    >
                      {formats.map((option) => (
                        <option key={option.label} value={option.label}>
                          {option.name}
                          {option.label === currentFormat ? "（当前格式）" : ""}
                        </option>
                      ))}
                    </Select>
                  </Field>

                  <Field
                    label="目标 Minecraft 版本"
                    hint="换格式不改方块。换版本只做确定的几件事（个别方块改名、容器物品格式），其余原样保留，转换后会列出需要你检查的地方。"
                    validationState={customValid ? "none" : "error"}
                    validationMessage={customValid ? undefined : "数据版本是 1 到 1000000 之间的整数"}
                  >
                    <Select aria-label="目标版本" value={version} onChange={(_, data) => setVersion(data.value)}>
                      <option value={KEEP}>
                        保持原版本{currentDataVersion === null ? "（文件里没写）" : `（数据版本 ${currentDataVersion}）`}
                      </option>
                      {(targets.data?.versions ?? []).map((option) => (
                        <option key={option.dataVersion} value={String(option.dataVersion)}>
                          {option.name}（{option.dataVersion}）
                        </option>
                      ))}
                      <option value={CUSTOM}>其他数据版本…</option>
                    </Select>
                  </Field>
                  {version === CUSTOM ? (
                    <Field label="数据版本号">
                      <Input
                        aria-label="数据版本号"
                        inputMode="numeric"
                        value={custom}
                        onChange={(_, data) => setCustom(data.value)}
                      />
                    </Field>
                  ) : null}
                </>
              ) : (
                <>
                  <MessageBar intent="success" role="status">
                    <MessageBarBody>已保存为「{result.fileName}」。</MessageBarBody>
                  </MessageBar>
                  {result.renamedBlocks > 0 ? <Text size={200}>{result.renamedBlocks.toLocaleString()} 个方块换了新 ID。</Text> : null}
                  {result.warnings.length > 0 ? (
                    <div className="convert-warnings">
                      <Text weight="semibold" size={200}>
                        需要你检查
                      </Text>
                      <ul>
                        {result.warnings.map((warning) => (
                          <li key={warning}>
                            <Text size={200}>{warning}</Text>
                          </li>
                        ))}
                      </ul>
                    </div>
                  ) : null}
                </>
              )}

              {error ? (
                <MessageBar intent="error" role="alert">
                  <MessageBarBody>{error}</MessageBarBody>
                </MessageBar>
              ) : null}
            </DialogContent>
            <DialogActions>
              {result === null ? (
                <>
                  <Button appearance="secondary" disabled={busy} onClick={onClose}>
                    取消
                  </Button>
                  <Button type="submit" appearance="primary" disabled={busy || chosenFormat === null || !customValid}>
                    转换
                  </Button>
                </>
              ) : (
                <Button appearance="primary" onClick={onClose}>
                  完成
                </Button>
              )}
            </DialogActions>
          </DialogBody>
        </form>
      </DialogSurface>
    </Dialog>
  );
}
