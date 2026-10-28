import { Text, Tooltip } from "@fluentui/react-components";
import { Navigation24Regular } from "@fluentui/react-icons";
import { AccountAvatar } from "../components/AccountAvatar";
import { mainPages, settingsPage } from "../navigation";
import type { PageDefinition } from "../navigation";
import { useAccounts } from "../state/accounts";
import { useNavigation } from "../state/navigation";

type NavPaneProps = {
  collapsed: boolean;
  onToggle: () => void;
  version: string;
};

/**
 * 导航栏，按 Fluent 的 NavigationView 排布：顶部汉堡按钮负责收起／展开，
 * 中间是主要页面，底部是当前账户和设置。收起时只剩图标，悬停显示名称。
 * 选中项左侧的指示条在切换时有一个短动画（见 `shell.css`）。
 */
export function NavPane({ collapsed, onToggle, version }: NavPaneProps) {
  const { current, navigate } = useNavigation();
  const { active } = useAccounts();

  const renderItem = (page: PageDefinition) => {
    const selected = page.id === current;
    const button = (
      <button
        key={page.id}
        type="button"
        className={`nav-item${selected ? " selected" : ""}`}
        aria-label={page.label}
        aria-current={selected ? "page" : undefined}
        onClick={() => navigate(page.id)}
      >
        <span className="nav-icon" aria-hidden="true">
          {selected ? page.activeIcon : page.icon}
        </span>
        <span className="nav-label">{page.label}</span>
      </button>
    );
    return collapsed ? (
      <Tooltip key={page.id} content={page.label} relationship="label" positioning="after">
        {button}
      </Tooltip>
    ) : (
      button
    );
  };

  const accountButton = (
    <button
      type="button"
      className="nav-item nav-account"
      aria-label={active ? `当前账户 ${active.name}` : "还没有账户"}
      onClick={() => navigate("accounts")}
    >
      <span className="nav-icon" aria-hidden="true">
        <AccountAvatar name={active?.name ?? null} size={24} />
      </span>
      <span className="nav-label nav-account-text">
        <Text size={300} weight="semibold" truncate wrap={false}>
          {active?.name ?? "添加账户"}
        </Text>
        <Text size={100} className="nav-account-kind">
          {active ? "离线账户" : "还没有账户"}
        </Text>
      </span>
    </button>
  );

  return (
    <nav
      className={`navpane${collapsed ? " collapsed" : ""}`}
      aria-label="主要导航"
      data-collapsed={collapsed}
    >
      <Tooltip
        content={collapsed ? "展开导航" : "收起导航"}
        relationship="label"
        positioning="after"
      >
        <button
          type="button"
          className="nav-item nav-toggle"
          aria-label={collapsed ? "展开导航" : "收起导航"}
          aria-expanded={!collapsed}
          onClick={onToggle}
        >
          <span className="nav-icon" aria-hidden="true">
            <Navigation24Regular />
          </span>
        </button>
      </Tooltip>

      <div className="nav-group">{mainPages.map(renderItem)}</div>

      <div className="nav-footer">
        {collapsed ? (
          <Tooltip
            content={active ? `当前账户：${active.name}` : "添加账户"}
            relationship="label"
            positioning="after"
          >
            {accountButton}
          </Tooltip>
        ) : (
          accountButton
        )}
        {renderItem(settingsPage)}
        <Text size={100} className="nav-version">
          v{version}
        </Text>
      </div>
    </nav>
  );
}
