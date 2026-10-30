# 模组加载器安装

在版本详情页选一个 Forge、NeoForge 或 Fabric 的版本，页面上唯一的「安装」按钮就先装原版、再装加载器、最后建实例。

## 分工

- `aether-modloader` 只算**要做什么**：解析官方元数据，产出 `ModLoaderInstallPlan`（一串 `InstallStep`）。它不联网、不写盘、不起进程。
- `src-tauri/src/commands/modloader/` 执行计划：联网、落盘、跑 installer。
- 前端只传「哪个加载器 / 哪个游戏版本 / 哪个加载器版本 / 实例名」。所有 URL 都由后端按官方源拼出来，前端没有办法让它请求任意地址，也没有任何路径。

## 流程

1. 先检查实例名（重名、不合法在下载之前就失败）。
2. 按原版流程装好游戏本体，**不建实例**。
3. 逐条执行加载器计划：下载库、做 installer 的安装（Forge / NeoForge，见下一节）、写版本 JSON。
4. 全部成功之后才建实例，版本直接指向加载器的版本 id。

实例放在最后建：中途失败（网络、installer 报错、取消）不会留下一个指向原版、名字却写着加载器的实例。进度事件的阶段是 `metadata`、`files`、`assets`、`modLoader`、`finalize`，进度条单调向前。

## Forge / NeoForge：不再把 installer 当黑盒运行

以前是 `java -jar installer.jar --installClient`：把下载来的程序整个交出去执行，它下载什么、写什么、改哪个文件启动器都管不到，还要求目录里已经有官方启动器写的 `launcher_profiles.json`。现在 `profile_install.rs` 读 installer 里的安装描述，**自己执行**：

1. 读 `install_profile.json` 和它指向的版本 JSON（都在 installer jar 里；installer 本身先按官方 maven 的 `.sha1` 校验过）。
2. 描述里列的每个库都带 SHA-1：下载并逐个校验；installer 自带的库（`url` 为空）从 jar 的 `maven/` 里取出来校验。**没有 SHA-1 的库直接拒绝。**
3. 按描述里的顺序运行「处理器」（对 Minecraft 客户端打补丁的 Java 程序，例如 `binarypatcher`）。处理器的 jar 和 classpath 都是第 2 步校验过的库；描述里给了输出的 SHA-1 的，运行后核对；输出已经在且匹配的处理器不重跑。
4. 最后写版本 JSON。它在，就说明前面都成功了。

和官方 installer 的区别：每个文件都有校验值才会被使用或执行；不碰 `launcher_profiles.json`；路径都经过 `is_safe_relative_path`；进度和取消由启动器控制（取消会杀掉正在跑的处理器）。

真实环境验证过：Forge 1.20.1-47.4.0 和 NeoForge 21.1.72 的客户端安装（`cargo test -p aether-desktop --lib -- --ignored live_`，会联网并需要 Java 17+/21+）。

处理器仍然是 Java 程序，它们自己可能联网（例如下载 Mojang 的混淆映射）。我们只能保证它们的 jar 是校验过的，管不到它们在运行时做什么。

## 下载怎么校验

每个下载的文件都必须有校验值，没有就不装：

- 计划里带 SHA-1 的（Fabric `launcherMeta` 里的库）用它；
- 没有的（Forge / NeoForge 的 installer，Fabric 的核心库）从官方 maven 的 `<文件>.sha1` 旁注文件读。读不到、格式不对一律失败；
- 先写到 `<目标>.part`，大小和 SHA-1 都通过后才改名到位，失败或中断不会在最终路径上留下残缺或被替换的文件；
- 连接失败、超时和服务器 5xx 最多重试 3 次（一个加载器要下几十到上百个文件，偶尔抖一下很常见）；404 和哈希不对不重试；
- 目标路径和原版安装共用同一条规则（`aether_download::is_safe_relative_path`），只认 https，单个文件有大小上限。

这些加载器没有像 Mojang 那样的「带哈希的官方清单」。旁注文件和构件在同一个 https 主机上：它防的是传输损坏和落盘错误，防不了仓库本身被攻破。

## 来源

只用官方：`meta.fabricmc.net`、`maven.fabricmc.net`、`maven.minecraftforge.net`、`maven.neoforged.net`。不做镜像切换。

## 还没做的

- **Quilt**：版本列表接口还没接，界面上列出但不可选。
- **Fabric API**：装 Fabric 之后大多数 Fabric 模组还需要它。界面只提示「不会自动安装」，不假装会装。要做的话需要先决定 mods 放在哪、下载校验值从哪来。
- **老格式的 Forge installer**（`install` + `versionInfo`，约 2017 年之前的版本）：会明确报错，不会悄悄装坏。1.12.2 的最后几个版本已经是新格式，可以装。
- 用安装出来的 Forge / NeoForge 实例**真正启动游戏**还没在真实环境里走过；安装本身（所有库校验通过、处理器跑完、输出核对）已经走过。

## 待定（需要维护者决定）

- **旁注 `.sha1` 作为 installer 本身的信任根。** 见上面「下载怎么校验」。installer 之后的一切都由它里面的 SHA-1 把关，所以这一个文件的来源就是整条链的起点。
- **新实例的默认设置。** 创建实例时会写 `options.txt`（只在文件不存在时写）：`lang`（中文）、`onboardAccessibility:false`、`tutorialStep:none`、`skipMultiplayerWarning:true`。是否默认写中文、是否替玩家跳过新手提示和多人游戏警告，是产品决定。
