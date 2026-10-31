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
import type {
  ContentItem,
  ContentKind,
  ContentLookup,
  DatapackInfo,
  InstanceDetails,
  LogContent,
  LogEntry,
  WorldDetails,
} from "../ipc/manage";
import type { BrowserLoginInfo, DeviceCodeInfo, LoginEvent, MicrosoftAccount } from "../ipc/accounts";
import type { SystemMemory } from "../ipc/system";
import type { ConversionResult, SchematicFormat } from "../ipc/schematics";
import { mockConversionTargets, mockSchematicView } from "./mockSchematics";

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

function startLogin(): void {
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
}

function beginLogin(): DeviceCodeInfo {
  startLogin();
  return { userCode: "MOCK1234", verificationUri: "https://www.microsoft.com/link", expiresInSecs: 900 };
}

function beginBrowserLogin(): BrowserLoginInfo {
  startLogin();
  return { expiresInSecs: 600 };
}

// ---- 实例管理 ------------------------------------------------------------------------------

const contentKindList: ContentKind[] = ["mods", "resourcepacks", "shaderpacks", "saves", "schematics"];

const file = (name: string, sizeBytes: number, enabled = true): ContentItem => ({ name, sizeBytes, enabled, isFolder: false });
const folder = (name: string, sizeBytes: number): ContentItem => ({ name, sizeBytes, enabled: true, isFolder: true });

function seedContent(): Record<ContentKind, ContentItem[]> {
  return {
    mods: [file("sodium-fabric-0.6.0.jar", 1_240_000), file("lithium-fabric-0.14.0.jar.disabled", 880_000, false), file("iris-1.8.0.jar", 2_400_000)],
    resourcepacks: [file("Faithful 32x.zip", 14_200_000), folder("我的资源包", 3_100_000)],
    shaderpacks: [file("BSL_v8.2.zip", 1_900_000)],
    saves: [folder("新的世界", 84_000_000), folder("创造测试", 12_500_000)],
    schematics: [
      file("Small House.litematic", 485),
      file("Watch Tower.litematic", 446),
      file("Pixel Heart.litematic", 377),
      file("Checker Platform 16.litematic", 399),
    ],
  };
}

const mockDatapacks: DatapackInfo[] = [
  { name: "Better Trading.zip", isFolder: false, sizeBytes: 48_000, description: "Villagers trade more fairly", packFormat: 48, state: "enabled" },
  { name: "我的数据包", isFolder: true, sizeBytes: 3_400, description: "自己写的几个函数", packFormat: 41, state: "enabled" },
  { name: "Old Recipes.zip", isFolder: false, sizeBytes: 9_100, description: null, packFormat: 26, state: "disabled" },
  { name: "Just Downloaded.zip", isFolder: false, sizeBytes: 120_000, description: "Not loaded by the game yet", packFormat: 48, state: "unknown" },
];

function mockWorld(name: string): WorldDetails {
  return {
    name,
    levelName: name,
    gameVersion: "1.21.1",
    dataVersion: 3955,
    gameMode: "survival",
    hardcore: false,
    difficulty: "normal",
    lastPlayed: 1_761_900_000,
    sizeBytes: 84_000_000,
    icon: null,
    datapacks: [...mockDatapacks],
    datapacksTruncated: false,
    unreadable: null,
  };
}

const contentByInstance = new Map<string, Record<ContentKind, ContentItem[]>>();

function contentOf(id: string): Record<ContentKind, ContentItem[]> {
  if (!instances.some((instance) => instance.id === id)) {
    throw "cannot open the instance: instance not found";
  }
  let content = contentByInstance.get(id);
  if (content === undefined) {
    content = seedContent();
    contentByInstance.set(id, content);
  }
  return content;
}

const mockPlatformInfo: Record<string, { slug: string; title: string; summary: string; version: string }> = {
  "sodium-fabric-0.6.0.jar": { slug: "sodium", title: "Sodium", summary: "A modern rendering engine and optimization mod for Minecraft.", version: "0.6.0" },
  "lithium-fabric-0.14.0.jar.disabled": { slug: "lithium", title: "Lithium", summary: "A Fabric mod that optimizes the game server without changing behavior.", version: "0.14.0" },
  "iris-1.8.0.jar": { slug: "iris", title: "Iris Shaders", summary: "A modern shaders mod for Minecraft compatible with existing OptiFine shader packs.", version: "1.8.0" },
  "BSL_v8.2.zip": { slug: "bsl-shaders", title: "BSL Shaders", summary: "Highly customizable shaders with realistic water and soft shadows.", version: "8.2" },
};

/** 一张画出来的 64×64 皮肤（只有头），给浏览器预览里的账户头像用。没有画布时没有皮肤。 */
function mockSkin(): string | null {
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const context = canvas.getContext("2d");
  if (context === null) {
    return null;
  }
  const rect = (x: number, y: number, w: number, h: number, colour: string) => {
    context.fillStyle = colour;
    context.fillRect(x, y, w, h);
  };
  // 里层的头：六个面都是肤色，顶面和正面的上两行是头发，侧面的上半是头发
  rect(0, 0, 32, 16, "#c58c6b");
  rect(8, 0, 8, 8, "#3b2a1a");
  rect(8, 8, 8, 2, "#3b2a1a");
  rect(0, 8, 8, 4, "#3b2a1a");
  rect(16, 8, 8, 4, "#3b2a1a");
  rect(24, 8, 8, 6, "#3b2a1a");
  rect(9, 12, 2, 1, "#ffffff");
  rect(13, 12, 2, 1, "#ffffff");
  rect(10, 12, 1, 1, "#4a4ae0");
  rect(13, 12, 1, 1, "#4a4ae0");
  rect(11, 15, 2, 1, "#8a4a3a");
  // 外层（帽子）：一条绿色的帽檐和侧面的一圈，其余透明
  rect(40, 8, 8, 2, "#1bd96a");
  rect(32, 8, 8, 2, "#1bd96a");
  rect(48, 8, 8, 2, "#1bd96a");
  rect(56, 8, 8, 2, "#1bd96a");
  rect(40, 0, 8, 8, "#1bd96a");
  return canvas.toDataURL("image/png");
}

function mockIcon(title: string): string {
  const letter = title.slice(0, 1).toUpperCase();
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="12" fill="#1bd96a"/><text x="32" y="44" font-size="34" font-family="sans-serif" text-anchor="middle" fill="#0b2a18">${letter}</text></svg>`;
  return `data:image/svg+xml;base64,${btoa(svg)}`;
}

function mockIdentify(id: string, kind: ContentKind): ContentLookup {
  const files = contentOf(id)[kind];
  return {
    items: files
      .filter((item) => mockPlatformInfo[item.name] !== undefined)
      .map((item) => {
        const info = mockPlatformInfo[item.name];
        return {
          name: item.name,
          platform: "modrinth" as const,
          projectSlug: info.slug,
          title: info.title,
          summary: info.summary,
          version: info.version,
          icon: info.slug === "lithium" ? null : mockIcon(info.title),
        };
      }),
    unreachable: false,
  };
}

function instanceDetails(id: string): InstanceDetails {
  const instance = instances.find((item) => item.id === id);
  if (instance === undefined) {
    throw "cannot open the instance: instance not found";
  }
  const content = contentOf(id);
  const counts = Object.fromEntries(contentKindList.map((kind) => [kind, content[kind].length])) as InstanceDetails["counts"];
  const sizeBytes = contentKindList.flatMap((kind) => content[kind]).reduce((sum, item) => sum + item.sizeBytes, 0);
  return { ...instance, loader: instance.name.includes("Fabric") ? "Fabric 0.16.5" : null, sizeBytes, counts };
}

function mockLogs(): LogEntry[] {
  return [
    { kind: "latest", name: "latest.log", sizeBytes: 4_200, modified: 1_761_800_000 },
    { kind: "crash", name: "crash-2026-10-28_20.18.51-client.txt", sizeBytes: 9_800, modified: 1_761_680_000 },
  ];
}

function mockLog(kind: string): LogContent {
  return kind === "crash"
    ? {
        text: "---- Minecraft Crash Report ----\nDescription: Loading library LWJGL system\n\njava.lang.UnsatisfiedLinkError: Failed to locate library: lwjgl.dll\n\nPlayer: <USERNAME:1>\n",
        truncated: false,
      }
    : {
        text: "[18:24:01] [main/INFO]: Setting user: <USERNAME:1>\n[18:24:03] [Render thread/INFO]: Reloading ResourceManager\n[18:24:07] [Render thread/INFO]: Stopping!\n",
        truncated: false,
      };
}

function removeFrom(id: string, kind: ContentKind, name: string) {
  const content = contentOf(id);
  if (!content[kind].some((item) => item.name === name)) {
    throw "that item does not exist";
  }
  content[kind] = content[kind].filter((item) => item.name !== name);
}

function toggleMod(id: string, name: string, enabled: boolean) {
  const content = contentOf(id);
  const item = content.mods.find((candidate) => candidate.name === name);
  if (item === undefined) {
    throw "that item does not exist";
  }
  if (item.enabled === enabled) {
    return;
  }
  const next = enabled ? name.replace(/\.disabled$/, "") : `${name}.disabled`;
  if (content.mods.some((candidate) => candidate.name === next)) {
    throw "a file with the new name already exists";
  }
  item.name = next;
  item.enabled = enabled;
}

function convertMock(id: string, name: string, format: SchematicFormat, dataVersion: number | null): ConversionResult {
  const content = contentOf(id);
  if (!content.schematics.some((item) => item.name === name)) {
    throw "that item does not exist";
  }
  const stem = name.replace(/\.[^.]+$/, "");
  let fileName = `${stem} (converted).${format}`;
  for (let attempt = 2; content.schematics.some((item) => item.name === fileName); attempt += 1) {
    fileName = `${stem} (converted) ${attempt}.${format}`;
  }
  content.schematics.push(file(fileName, 500));
  return {
    fileName,
    format,
    dataVersion: dataVersion ?? 3955,
    renamedBlocks: dataVersion !== null && dataVersion >= 3698 ? 2 : 0,
    warnings:
      dataVersion === null
        ? []
        : ["Blocks are not checked against the target version's block list; a block that does not exist there will not load."],
  };
}

function renameMock(id: string, name: string): InstanceSummary {
  const instance = instances.find((item) => item.id === id);
  if (instance === undefined) {
    throw "cannot open the instance: instance not found";
  }
  const trimmed = name.trim();
  if (instances.some((item) => item.id !== id && item.name.toLowerCase() === trimmed.toLowerCase())) {
    throw "an instance with this name already exists";
  }
  const renamed = { ...instance, name: trimmed };
  instances = instances.map((item) => (item.id === id ? renamed : item));
  saveInstances();
  return renamed;
}

function deleteMock(id: string) {
  if (games.get(id)?.state === "running") {
    throw "stop the game before deleting this instance";
  }
  contentOf(id);
  instances = instances.filter((item) => item.id !== id);
  contentByInstance.delete(id);
  saveInstances();
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
    case "begin_microsoft_browser_login":
      return beginBrowserLogin();
    case "cancel_microsoft_login": {
      const was = loginRunning;
      loginCancelled = loginRunning;
      return was;
    }
    case "open_microsoft_verification_page":
      return null;
    case "list_microsoft_accounts":
      return accounts();
    case "account_skin":
      return mockSkin();
    case "remove_microsoft_account":
      writeJson(
        accountsKey,
        accounts().filter((account) => account.id !== args?.id),
      );
      return null;
    case "instance_details":
      return instanceDetails(String(args?.id));
    case "rename_installed_instance":
      return renameMock(String(args?.id), String(args?.name));
    case "delete_installed_instance":
      deleteMock(String(args?.id));
      return null;
    case "list_instance_content":
      return contentOf(String(args?.id))[args?.kind as ContentKind] ?? [];
    case "identify_instance_content":
      await sleep(400);
      return mockIdentify(String(args?.id), args?.kind as ContentKind);
    case "open_content_project":
      return null;
    case "set_instance_mod_enabled":
      toggleMod(String(args?.id), String(args?.name), Boolean(args?.enabled));
      return null;
    case "remove_instance_content":
      removeFrom(String(args?.id), args?.kind as ContentKind, String(args?.name));
      return null;
    case "list_instance_logs":
      contentOf(String(args?.id));
      return mockLogs();
    case "read_instance_log":
      contentOf(String(args?.id));
      return mockLog(String(args?.kind));
    case "open_instance_folder":
      contentOf(String(args?.id));
      return null;
    case "get_world_details":
      contentOf(String(args?.id));
      return mockWorld(String(args?.world));
    case "remove_world_datapack": {
      const pack = mockDatapacks.findIndex((item) => item.name === args?.name);
      if (pack < 0) {
        throw "no such data pack";
      }
      mockDatapacks.splice(pack, 1);
      return null;
    }
    case "open_world_folder":
      return null;
    case "read_instance_schematic": {
      const content = contentOf(String(args?.id));
      if (!content.schematics.some((item) => item.name === args?.name)) {
        throw "that item does not exist";
      }
      return mockSchematicView(String(args?.name));
    }
    case "convert_instance_schematic":
      return convertMock(
        String(args?.id),
        String(args?.name),
        args?.format as SchematicFormat,
        (args?.dataVersion as number | null) ?? null,
      );
    case "schematic_conversion_targets":
      return mockConversionTargets;
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
