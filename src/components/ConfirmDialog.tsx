import {
  Button,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  MessageBar,
  MessageBarBody,
  Spinner,
} from "@fluentui/react-components";
import type { ReactNode } from "react";

type ConfirmDialogProps = {
  open: boolean;
  title: string;
  body: ReactNode;
  confirmLabel: string;
  /** 不可恢复的操作：确认按钮用红色，并且焦点默认落在「取消」上 */
  danger?: boolean;
  busy?: boolean;
  /** 上一次尝试失败的原因 */
  error?: string | null;
  onConfirm: () => void;
  onCancel: () => void;
};

/** 删除之类的确认对话框。点遮罩、按 Esc、点「取消」都是取消；操作进行中不能关。 */
export function ConfirmDialog({
  open,
  title,
  body,
  confirmLabel,
  danger = false,
  busy = false,
  error = null,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  return (
    <Dialog
      open={open}
      modalType="alert"
      onOpenChange={(_, data) => {
        if (!data.open && !busy) {
          onCancel();
        }
      }}
    >
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{title}</DialogTitle>
          <DialogContent className="dialog-content">
            {body}
            {error ? (
              <MessageBar intent="error" role="alert">
                <MessageBarBody>{error}</MessageBarBody>
              </MessageBar>
            ) : null}
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" disabled={busy} onClick={onCancel} autoFocus={danger}>
              取消
            </Button>
            <Button
              appearance="primary"
              className={danger ? "danger-action" : undefined}
              disabled={busy}
              onClick={onConfirm}
              icon={busy ? <Spinner size="tiny" /> : undefined}
            >
              {confirmLabel}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
