import { Text } from "@fluentui/react-components";
import { AccountAvatar } from "../components/AccountAvatar";
import { mainPages, settingsPage } from "../navigation";
import type { PageDefinition } from "../navigation";
import { useAccounts } from "../state/accounts";
import { useNavigation } from "../state/navigation";

type NavPaneProps = {
  version: string;
};

/**
 * 导航栏：在标题栏下面一横排。左边是主要页面（主页、实例、下载），右边是当前账户、设置和版本号；
 * 当前账户（头像和名字）就是进账户页的入口，没有单独的「账户」按钮。选中项的
 * 下边有一条指示条，切换时从中间向两边展开（见 `shell.css`）。窗口窄了只留图标，名称
 * 放在 `title` 里，读屏用的是 `aria-label`。
 */
export function NavPane({ version }: NavPaneProps) {
  const { current, navigate } = useNavigation();
  const { active } = useAccounts();

  const renderItem = (page: PageDefinition) => {
    const selected = page.id === current;
    return (
      <button
        key={page.id}
        type="button"
        className={`nav-item${selected ? " selected" : ""}`}
        aria-label={page.label}
        aria-current={selected ? "page" : undefined}
        title={page.label}
        onClick={() => navigate(page.id)}
      >
        <span className="nav-icon" aria-hidden="true">
          {selected ? page.activeIcon : page.icon}
        </span>
        <span className="nav-label">{page.label}</span>
      </button>
    );
  };

  return (
    <nav className="navbar" aria-label="主要导航">
      <div className="nav-group">{mainPages.map(renderItem)}</div>

      <div className="nav-footer">
        <button
          type="button"
          className={`nav-item nav-account${current === "accounts" ? " selected" : ""}`}
          aria-label={active ? `当前账户 ${active.name}` : "还没有账户"}
          aria-current={current === "accounts" ? "page" : undefined}
          title={active ? `当前账户：${active.name}（点开管理账户）` : "添加账户"}
          onClick={() => navigate("accounts")}
        >
          <span className="nav-icon" aria-hidden="true">
            <AccountAvatar
              name={active?.name ?? null}
              uuid={active?.kind === "microsoft" ? active.uuid : null}
              size={32}
            />
          </span>
          <span className="nav-label nav-account-text">
            <Text size={300} weight="semibold" truncate wrap={false}>
              {active?.name ?? "添加账户"}
            </Text>
            <Text size={200} className="nav-account-kind">
              {active ? (active.kind === "microsoft" ? "微软账户" : "离线账户") : "还没有账户"}
            </Text>
          </span>
        </button>
        {renderItem(settingsPage)}
        <Text size={100} className="nav-version">
          v{version}
        </Text>
      </div>
    </nav>
  );
}
