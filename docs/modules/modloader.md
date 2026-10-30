# 模组加载器安装

在版本详情页选一个 Forge、NeoForge 或 Fabric 的版本，页面上唯一的「安装」按钮就先装原版、再装加载器、最后建实例。

## 分工

- `aether-modloader` 只算**要做什么**：解析官方元数据，产出 `ModLoaderInstallPlan`（一串 `InstallStep`）。它不联网、不写盘、不起进程。
- `src-tauri/src/commands/modloader/` 执行计划：联网、落盘、跑 installer。
- 前端只传「哪个加载器 / 哪个游戏版本 / 哪个加载器版本 / 实例名」。所有 URL 都由后端按官方源拼出来，前端没有办法让它请求任意地址，也没有任何路径。

## 流程

1. 先检查实例名（重名、不合法在下载之前就失败）。
2. 按原版流程装好游戏本体，**不建实例**。
3. 逐条执行加载器计划：下载库、跑 installer（Forge / NeoForge）、写版本 JSON。
4. 全部成功之后才建实例，版本直接指向加载器的版本 id。

实例放在最后建：中途失败（网络、installer 报错、取消）不会留下一个指向原版、名字却写着加载器的实例。进度事件的阶段是 `metadata`、`files`、`assets`、`modLoader`、`finalize`，进度条单调向前。

## 下载怎么校验

每个下载的文件都必须有校验值，没有就不装：

- 计划里带 SHA-1 的（Fabric `launcherMeta` 里的库）用它；
- 没有的（Forge / NeoForge 的 installer，Fabric 的核心库）从官方 maven 的 `<文件>.sha1` 旁注文件读。读不到、格式不对一律失败；
- 先写到 `<目标>.part`，大小和 SHA-1 都通过后才改名到位，失败或中断不会在最终路径上留下残缺或被替换的文件；
- 目标路径和原版安装共用同一条规则（`aether_download::is_safe_relative_path`），只认 https，单个文件有大小上限。

这些加载器没有像 Mojang 那样的「带哈希的官方清单」。旁注文件和构件在同一个 https 主机上：它防的是传输损坏和落盘错误，防不了仓库本身被攻破。

## 来源

只用官方：`meta.fabricmc.net`、`maven.fabricmc.net`、`maven.minecraftforge.net`、`maven.neoforged.net`。不做镜像切换。

## 还没做的

- **Quilt**：版本列表接口还没接，界面上列出但不可选。
- **Fabric API**：装 Fabric 之后大多数 Fabric 模组还需要它。界面只提示「不会自动安装」，不假装会装。要做的话需要先决定 mods 放在哪、下载校验值从哪来。
- Forge / NeoForge 的 installer 没有在真实环境里端到端跑过；Fabric 在真实环境里跑通过。

## 待定（需要维护者决定）

- **运行 Forge / NeoForge 的 installer。** 它是官方的 Java 程序，会自己再下载库并写文件，这些下载由它自己校验，启动器管不到。要不要接受「执行官方 installer」这个信任模型，还是改成自己实现安装步骤，是设计问题。
- **旁注 `.sha1` 作为信任根。** 见上面「下载怎么校验」。
- **新实例的默认设置。** 创建实例时会写 `options.txt`（只在文件不存在时写）：`lang`（中文）、`onboardAccessibility:false`、`tutorialStep:none`、`skipMultiplayerWarning:true`。是否默认写中文、是否替玩家跳过新手提示和多人游戏警告，是产品决定。
