/**
 * 浏览器预览用的假后端。
 *
 * `npm run dev` 在普通浏览器里打开时没有 Tauri，页面上的实例、版本目录、登录都是空的，
 * 没法看也没法测。这里在页面加载前装一个假的 `__TAURI_INTERNALS__`，把全部命令路由到
 * 内存里的假数据，让前端走和桌面版完全相同的代码路径（`isTauri()` 也变为 true）。
 *
 * 只在开发服务器里加载（见 `main.tsx`），不会进入生产构建。网址上带 `?mock=off`
 * 可以回到「没有后端」的原始浏览器预览；`?mock=login-denied`、`?mock=login-child`、
 * `?mock=login-expired` 让微软登录以对应的原因失败，`?mock=no-login` 模拟没有配置
 * Azure 应用 ID 的构建。
 *
 * 假数据里没有任何真实令牌，登录出来的账户是虚构的。
 */

import type { InstallProgress } from "../ipc/install";
import type { InstanceSummary } from "../ipc/instances";
import type { GameRunStatus } from "../ipc/launch";
import type { MinecraftCatalogVersion, MinecraftVersionCatalog } from "../ipc/catalog";
import type { ImportResult, ImportSource, LauncherInstallation } from "../ipc/import";
import type { JavaRuntimeInfo } from "../ipc/java";
import type { DeviceCodeInfo, LoginEvent, MicrosoftAccount } from "../ipc/accounts";
import type { SystemMemory } from "../ipc/system";

type Args = Record<string, unknown> | undefined;
type Listener = { event: string; handlerId: number };

const accountsKey = "aether.mock.microsoft";
const instancesKey = "aether.mock.instances";
const loginDelayMs = 6000;

const mode = new URLSearchParams(window.location.search).get("mock") ?? "on";

const callbacks = new Map<number, (event: { event: string; id: number; payload: unknown }) => void>();
const listeners = new Map<number, Listener>();
let nextId = 1;

function emit(event: string, payload: unknown) {
  for (const [id, listener] of listeners) {
    if (listener.event === event) {
      callbacks.get(listener.handlerId)?.({ event, id, payload });
    }
  }
}

const sleep = (ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms));

function readJson<T>(key: string, fallback: T): T {
  try {
    const raw = window.localStorage.getItem(key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

function writeJson(key: string, value: unknown) {
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // 存储不可用时只在本次页面里生效
  }
}

function uuid(): string {
  return crypto.randomUUID();
}

// ---- 数据 --------------------------------------------------------------------------------

const defaultInstances: InstanceSummary[] = [
  { id: uuid(), name: "1.21.1", version: "1.21.1", state: "ready" },
  { id: uuid(), name: "1.20.4 生存", version: "1.20.4", state: "ready" },
  { id: uuid(), name: "1.19.4", version: "1.19.4", state: "incomplete" },
];
let instances = readJson<InstanceSummary[]>(instancesKey, defaultInstances);

function saveInstances() {
  writeJson(instancesKey, instances);
}

function catalog(): MinecraftVersionCatalog {
  const versions: MinecraftCatalogVersion[] = [];
  const add = (id: string, type: string, year: number, month: number) => {
    versions.push({
      id,
      type,
      url: `https://piston-meta.mojang.com/v1/packages/mock/${id}.json`,
      sha1: "0".repeat(40),
      releaseTime: new Date(Date.UTC(year, month, 1)).toISOString(),
    });
  };
  add("25w02a", "snapshot", 2025, 0);
  add("1.21.4", "release", 2024, 11);
  add("24w46a", "snapshot", 2024, 10);
  add("1.21.3", "release", 2024, 9);
  add("1.21.1", "release", 2024, 7);
  add("1.21", "release", 2024, 5);
  add("1.20.6", "release", 2024, 4);
  add("1.20.4", "release", 2023, 11);
  add("1.20.1", "release", 2023, 5);
  add("1.19.4", "release", 2023, 2);
  add("1.19.2", "release", 2022, 7);
  add("1.18.2", "release", 2022, 2);
  add("1.17.1", "release", 2021, 5);
  add("1.16.5", "release", 2021, 0);
  add("1.15.2", "release", 2020, 0);
  add("1.14.4", "release", 2019, 6);
  add("1.12.2", "release", 2017, 8);
  add("1.8.9", "release", 2015, 11);
  add("1.7.10", "release", 2014, 5);
  add("b1.7.3", "old_beta", 2011, 6);
  add("a1.2.6", "old_alpha", 2010, 11);
  return { latest: { release: "1.21.4", snapshot: "25w02a" }, versions };
}

const javaRuntimes: JavaRuntimeInfo[] = [
  { executable: "C:\\Program Files\\Java\\jdk-21\\bin\\java.exe", majorVersion: 21, architecture: "x86_64", vendor: "Eclipse Adoptium" },
  { executable: "C:\\Program Files\\Java\\jdk-17\\bin\\java.exe", majorVersion: 17, architecture: "x86_64", vendor: "Oracle" },
  { executable: "C:\\Program Files\\Java\\jre-8\\bin\\java.exe", majorVersion: 8, architecture: "x86_64", vendor: null },
];

const memory: SystemMemory = { totalMib: 16_269, availableMib: 3277 };

// ---- 安装 --------------------------------------------------------------------------------

let installing = false;
let installCancelled = false;

async function install(version: string, name: string): Promise<InstanceSummary> {
  if (installing) {
    throw "an installation is already running";
  }
  if (instances.some((instance) => instance.name.toLowerCase() === name.toLowerCase())) {
    throw "an instance with this name already exists";
  }
  installing = true;
  installCancelled = false;
  try {
    const progress = (stage: InstallProgress["stage"], done: number, total: number, bytes: number) =>
      emit("install://progress", {
        version,
        stage,
        filesDone: done,
        filesTotal: total,
        bytesDone: Math.round((done / total) * bytes),
        bytesTotal: bytes,
      } satisfies InstallProgress);

    const stages: [InstallProgress["stage"], number, number][] = [
      ["metadata", 1, 60_000],
      ["files", 24, 31_000_000],
      ["assets", 40, 160_000_000],
    ];
    for (const [stage, total, bytes] of stages) {
      for (let done = 0; done <= total; done += Math.max(1, Math.round(total / 24))) {
        if (installCancelled) {
          throw "cancelled";
        }
        progress(stage, Math.min(done, total), total, bytes);
        await sleep(70);
      }
      progress(stage, total, total, bytes);
    }
    progress("finalize", 1, 1, 0);
    await sleep(200);
    const created: InstanceSummary = { id: uuid(), name, version, state: "ready" };
    instances = [...instances, created];
    saveInstances();
    return created;
  } finally {
    installing = false;
  }
}

// ---- 游戏 --------------------------------------------------------------------------------

const games = new Map<string, GameRunStatus>();

function startGame(id: string, accountId: string | null): GameRunStatus {
  if (!instances.some((instance) => instance.id === id)) {
    throw "unknown instance";
  }
  if (accountId !== null && !accounts().some((account) => account.id === accountId)) {
    throw "microsoft-sign-in-required";
  }
  const status: GameRunStatus = {
    instanceId: id,
    state: "running",
    pid: 1000 + Math.floor(Math.random() * 60_000),
    exitCode: null,
    discardedLogs: 0,
  };
  games.set(id, status);
  return status;
}

// ---- 微软账户 ------------------------------------------------------------------------------

function accounts(): MicrosoftAccount[] {
  return readJson<MicrosoftAccount[]>(accountsKey, []);
}

let loginRunning = false;
let loginCancelled = false;

const loginFailures: Record<string, Extract<LoginEvent, { state: "failed" }>["failure"]> = {
  "login-denied": { reason: "denied", message: "the sign-in was declined" },
  "login-child": { reason: "child-account", message: "this is a child account" },
  "login-expired": { reason: "expired", message: "the sign-in code expired" },
};

function beginLogin(): DeviceCodeInfo {
  if (mode === "no-login") {
    throw "microsoft-not-configured";
  }
  if (loginRunning) {
    throw "a Microsoft sign-in is already in progress";
  }
  loginRunning = true;
  loginCancelled = false;
  emit("login://status", { state: "waiting" } satisfies LoginEvent);

  void sleep(loginDelayMs).then(() => {
    if (loginCancelled) {
      loginRunning = false;
      emit("login://status", { state: "cancelled" } satisfies LoginEvent);
      return;
    }
    loginRunning = false;
    const failure = loginFailures[mode];
    if (failure !== undefined) {
      emit("login://status", { state: "failed", failure } satisfies LoginEvent);
      return;
    }
    const account: MicrosoftAccount = {
      id: "069a79f4-44e9-4726-a5be-fca90e38aaf5",
      displayName: "MockPlayer",
      uuid: "069a79f444e94726a5befca90e38aaf5",
      xuid: "2535400000000001",
    };
    writeJson(accountsKey, [...accounts().filter((existing) => existing.id !== account.id), account]);
    emit("login://status", { state: "signed-in", account } satisfies LoginEvent);
  });

  return { userCode: "MOCK1234", verificationUri: "https://www.microsoft.com/link", expiresInSecs: 900 };
}

// ---- 导入 --------------------------------------------------------------------------------

const importSources: ImportSource[] = [
  {
    token: "mock-pcl2",
    sourceKind: "pcl2",
    sourcePath: "D:\\Games\\PCL2\\.minecraft",
    candidates: [
      { id: "1.20.1", displayName: "1.20.1" },
      { id: "1.16.5", displayName: "1.16.5" },
      { id: "1.20.1-fabric", displayName: "1.20.1-fabric-0.15.11" },
    ],
  },
];

const detected: LauncherInstallation[] = [
  { kind: "pcl2", launcherRoot: "D:\\Games\\PCL2", sources: importSources },
  {
    kind: "prism",
    launcherRoot: "C:\\Users\\player\\AppData\\Roaming\\PrismLauncher",
    sources: [
      {
        token: "mock-prism",
        sourceKind: "prism",
        sourcePath: "C:\\Users\\player\\AppData\\Roaming\\PrismLauncher",
        candidates: [
          { id: "survival", displayName: "生存（1.19.2）" },
          { id: "modded", displayName: "Fabric 整合包（1.20.1）" },
        ],
      },
    ],
  },
];

async function importVersions(token: string, ids: string[]): Promise<ImportResult> {
  const source = [...importSources, ...detected.flatMap((entry) => entry.sources)].find(
    (candidate) => candidate.token === token,
  );
  if (source === undefined) {
    throw "this import source is no longer available; choose the folder again";
  }
  const imported: InstanceSummary[] = [];
  const skipped: ImportResult["skipped"] = [];
  for (const [index, id] of ids.entries()) {
    if (/fabric|forge|modded/i.test(id)) {
      skipped.push({ id, reason: "needs-loader" });
      continue;
    }
    emit("import://item", { id, index, total: ids.length });
    for (let step = 0; step <= 10; step += 1) {
      emit("install://progress", {
        version: id,
        stage: "files",
        filesDone: step,
        filesTotal: 10,
        bytesDone: step * 3_000_000,
        bytesTotal: 30_000_000,
      } satisfies InstallProgress);
      await sleep(60);
    }
    const created: InstanceSummary = { id: uuid(), name: id, version: id, state: "ready" };
    instances = [...instances, created];
    imported.push(created);
  }
  saveInstances();
  return { imported, skipped };
}

// ---- 路由 --------------------------------------------------------------------------------

async function handle(command: string, args: Args): Promise<unknown> {
  switch (command) {
    case "application_info":
      return { name: "Aether Launcher", version: "0.1.0", coreStatus: "mock backend" };
    case "list_java_runtimes":
      return javaRuntimes;
    case "system_memory":
      return memory;
    case "list_minecraft_versions":
      await sleep(250);
      return catalog();
    case "list_instances":
      return { instances, skipped: [] };
    case "install_vanilla_version":
      return install(String(args?.version), String(args?.name));
    case "cancel_vanilla_install":
      installCancelled = installing;
      return installing;
    case "start_installed_instance":
      return startGame(String(args?.id), (args?.accountId as string | null) ?? null);
    case "stop_installed_instance": {
      const current = games.get(String(args?.id));
      if (current === undefined) {
        throw "no game was started in this session";
      }
      const stopped: GameRunStatus = { ...current, state: "stopped" };
      games.set(String(args?.id), stopped);
      return stopped;
    }
    case "game_status":
      return games.get(String(args?.id)) ?? null;
    case "microsoft_login_available":
      return mode !== "no-login";
    case "begin_microsoft_login":
      return beginLogin();
    case "cancel_microsoft_login": {
      const was = loginRunning;
      loginCancelled = loginRunning;
      return was;
    }
    case "open_microsoft_verification_page":
      return null;
    case "list_microsoft_accounts":
      return accounts();
    case "remove_microsoft_account":
      writeJson(
        accountsKey,
        accounts().filter((account) => account.id !== args?.id),
      );
      return null;
    case "pick_import_folder":
      await sleep(300);
      return importSources[0];
    case "detect_launcher_installations":
      await sleep(500);
      return detected;
    case "pick_launcher_folder":
      await sleep(300);
      return detected[0];
    case "import_versions":
      return importVersions(String(args?.token), (args?.ids as string[]) ?? []);
    default:
      throw `mock backend: unknown command ${command}`;
  }
}

/** 窗口插件和事件插件：行为类命令什么都不做，查询类命令给一个中性的值。 */
function handlePlugin(command: string, args: Args): unknown {
  switch (command) {
    case "plugin:event|listen": {
      const id = nextId++;
      listeners.set(id, { event: String(args?.event), handlerId: Number(args?.handler) });
      return id;
    }
    case "plugin:event|unlisten":
      listeners.delete(Number(args?.eventId));
      return null;
    case "plugin:window|is_maximized":
    case "plugin:window|is_fullscreen":
      return false;
    default:
      return null;
  }
}

export function installMockBackend() {
  const internals = {
    invoke(command: string, args?: Args): Promise<unknown> {
      if (command.startsWith("plugin:")) {
        return Promise.resolve(handlePlugin(command, args));
      }
      return handle(command, args);
    },
    transformCallback(callback: (event: never) => void): number {
      const id = nextId++;
      callbacks.set(id, callback as never);
      return id;
    },
    unregisterCallback(id: number) {
      callbacks.delete(id);
    },
    convertFileSrc: (path: string) => path,
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
  };
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: internals, configurable: true });
  Object.defineProperty(window, "__TAURI_EVENT_PLUGIN_INTERNALS__", {
    value: { unregisterListener: () => undefined },
    configurable: true,
  });

  const chip = document.createElement("div");
  chip.textContent = "模拟数据 · 网址加 ?mock=off 关闭";
  chip.setAttribute("role", "note");
  chip.style.cssText =
    "position:fixed;right:8px;bottom:8px;z-index:99999;padding:2px 8px;border-radius:10px;" +
    "font:11px system-ui,sans-serif;background:#7c4dff;color:#fff;opacity:.85;pointer-events:none";
  document.body.appendChild(chip);
}
