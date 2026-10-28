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
  Radio,
  RadioGroup,
  Text,
} from "@fluentui/react-components";
import { useState } from "react";
import type { FormEvent } from "react";
import {
  createOfflineAccount,
  describeUuidMode,
  maxPlayerNameLength,
  minPlayerNameLength,
  uuidModes,
} from "../lib/accounts";
import type { AccountDraftErrors, UuidMode } from "../lib/accounts";
import { useAccounts } from "../state/accounts";

type AddOfflineAccountDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

/**
 * 添加离线账户：填用户名，并选择 UUID 的生成方式（标准／随机／自定义）。
 * 离线账户只在本机使用，没有任何令牌，所以所有信息都可以直接看到。
 */
export function AddOfflineAccountDialog({ open, onOpenChange }: AddOfflineAccountDialogProps) {
  const { book, add } = useAccounts();
  const [name, setName] = useState("");
  const [mode, setMode] = useState<UuidMode>("standard");
  const [customUuid, setCustomUuid] = useState("");
  const [errors, setErrors] = useState<AccountDraftErrors>({});

  const close = () => {
    setName("");
    setMode("standard");
    setCustomUuid("");
    setErrors({});
    onOpenChange(false);
  };

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    const result = createOfflineAccount(book.accounts, { name, mode, customUuid });
    if (!result.ok) {
      setErrors(result.errors);
      return;
    }
    add(result.account);
    close();
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
        <form onSubmit={handleSubmit} noValidate>
          <DialogBody>
            <DialogTitle>添加离线账户</DialogTitle>
            <DialogContent className="dialog-content">
              <Field
                label="用户名"
                required
                hint={`${minPlayerNameLength}–${maxPlayerNameLength} 位，只能用字母、数字和下划线`}
                validationMessage={errors.name}
                validationState={errors.name ? "error" : "none"}
              >
                <Input
                  value={name}
                  autoFocus
                  placeholder="例如 Steve"
                  maxLength={maxPlayerNameLength + 4}
                  onChange={(_, data) => {
                    setName(data.value);
                    if (errors.name) {
                      setErrors((previous) => ({ ...previous, name: undefined }));
                    }
                  }}
                />
              </Field>

              <Field label="UUID 格式">
                <RadioGroup
                  value={mode}
                  onChange={(_, data) => {
                    setMode(data.value as UuidMode);
                    setErrors((previous) => ({ ...previous, uuid: undefined }));
                  }}
                >
                  {uuidModes.map((value) => (
                    <Radio key={value} value={value} label={describeUuidMode(value).label} />
                  ))}
                </RadioGroup>
              </Field>
              <Text size={200} className="muted">
                {describeUuidMode(mode).description}
              </Text>

              {mode === "custom" ? (
                <Field
                  label="自定义 UUID"
                  required
                  validationMessage={errors.uuid}
                  validationState={errors.uuid ? "error" : "none"}
                >
                  <Input
                    className="mono"
                    value={customUuid}
                    placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                    onChange={(_, data) => {
                      setCustomUuid(data.value);
                      if (errors.uuid) {
                        setErrors((previous) => ({ ...previous, uuid: undefined }));
                      }
                    }}
                  />
                </Field>
              ) : null}
            </DialogContent>
            <DialogActions>
              <Button appearance="secondary" type="button" onClick={close}>
                取消
              </Button>
              <Button appearance="primary" type="submit">
                添加
              </Button>
            </DialogActions>
          </DialogBody>
        </form>
      </DialogSurface>
    </Dialog>
  );
}
