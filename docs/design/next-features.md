# 下一批功能

2026-10-31 项目负责人提出的一批需求，按他的原话整理，加上每一条需要什么、依赖什么、哪些还没定。**「待定」的由负责人定，不在这里拍板。** 做完一条就把它从这里移走（进对应的 `docs/modules/` 文档）。

## 总览

| # | 需求 | 状态 | 先要有 |
| --- | --- | --- | --- |
| 1 | 导航栏改到页面顶部，不在左边 | 已做（见 [ui-shell.md](ui-shell.md)） | — |
| 2 | 原生界面：iced / egui / Slint 三个候选都做对比 | iced、egui 做完且输入法通过；Slint 在做 | — |
| 3 | 世界可管理：能查看数据包等 | 已做（见 [instance-management.md](../modules/instance-management.md)）；启用／停用数据包没做 | — |
| 4 | 下载页能从 Modrinth 和 CurseForge 下载模组、数据包、资源包、光影等，装进实例（或实例的世界）或选定文件夹 | Modrinth → 实例 / 世界已做（见 [content-metadata.md](../modules/content-metadata.md)）；整合包（`.mrpack`）已做（见 [modpack.md](../modules/modpack.md)）；进度和取消已做（见 [content-metadata.md](../modules/content-metadata.md)）；选定文件夹已做（走同一套进度、取消和排队，见 [content-metadata.md](../modules/content-metadata.md)「保存到所选文件夹」）；CurseForge 没做 | CurseForge 的 key（待定） |
| 5 | 皮肤库：选皮肤、加皮肤；换皮肤时有多个正版账户就弹卡片多选；下载页能从 NameMC / Skindex 等下皮肤，存进皮肤库或选定文件夹 | 皮肤库和「换上」已做（左边导航里的独立栏目「皮肤库」，见 [skin-library.md](../modules/skin-library.md)）；下载页的皮肤来源没做 | — |
| 6 | 第三方账户登录，像 LittleSkin | 登录、刷新、移除、启动已做（见 [yggdrasil.md](../modules/yggdrasil.md)） | — |
| 7 | i18n | 基础设施、跟随系统（中英）和导航栏已做（见 [i18n.md](i18n.md)）；其余页面分片迁移，后端错误码方案待定 | — |
| 8 | 拖放安装 | 分类、单次令牌、流式复制、确认界面和逐项报告已做；私有原生监听器待接入，当前不能实际拖入 | 私有原生监听器 |

## 3. 世界管理

存档页的每个世界能点开，看到：

- 基本信息，读 `level.dat`（压缩的 NBT，不可信输入：大小和深度有上限）：世界名、游戏版本、`DataVersion`、模式、难度、极限、上次游玩、体积、`icon.png`（只收 64 KiB 以内的 PNG）。
- **数据包**：`saves/<世界>/datapacks/` 里每一项（压缩包或文件夹）的名字、大小、`pack.mcmeta` 里的描述和 `pack_format`（压缩包只读这一个条目，条目数和大小有上限，不解压到磁盘）；是否启用按 `level.dat` 里的 `DataPacks.Enabled` / `Disabled` 判断，对不上的标「未知」。
- 动作：删除一个数据包、打开世界文件夹。游戏运行中不能改文件，和现在的内容管理一致。

后端契约（`aether-app::world`，字段 camelCase，没有任何路径）：

```text
WorldDetails { name, levelName?, gameVersion?, dataVersion?, gameMode?, hardcore?, difficulty?,
               lastPlayed? (Unix 秒), sizeBytes, icon? (data: PNG), datapacks[], datapacksTruncated,
               unreadable? (level.dat 为什么没读成) }
DatapackInfo { name, isFolder, sizeBytes, description?, packFormat?, state: enabled | disabled | unknown }
命令：get_world_details {id, world}、remove_world_datapack {id, world, name}、open_world_folder {id, world}
```

待定：启用 / 停用数据包要改 `level.dat`（游戏关着才安全），这一版不做。

## 4. 内容下载

类型：模组、数据包、资源包、光影，以及「更多」（整合包、世界、插件）。来源：Modrinth（不要 key）和 CurseForge（要 key）。

目标，三种：

1. **实例**：模组 → `mods/`，资源包 → `resourcepacks/`，光影 → `shaderpacks/`；
2. **实例的某个世界**：数据包 → `saves/<世界>/datapacks/`；
3. **选定的文件夹**：由后端开原生文件夹对话框（页面不传路径），只写校验过的文件，文件名净化，已存在的文件不覆盖；与装进实例共用操作 id、进度事件、取消和两槽队列，保留文件逐项报告，见 [内容元数据](../modules/content-metadata.md)「保存到所选文件夹」。

每个文件都按 API 给的哈希校验（Modrinth：sha1 + sha512；CurseForge：sha1），哈希不符就丢掉。搜索要按游戏版本、加载器、类型筛，并给出依赖。

待定：

- **CurseForge 的 key 放哪**。负责人要求先调研 PCL、PCL CE、HMCL 等是怎么做的，再定。调研结果出来之前不接 CurseForge，Modrinth 先做。已知的一条：PolyMC 曾把 key 写进构建文件，后来按 CurseForge 条款撤掉了（`docs/modules/content-metadata.md` 里的「待定」第 1 条）。
- `downloadUrl` 为 `null`（作者不让第三方分发）的 CurseForge 文件怎么处理：打开项目页让用户自己下载，还是隐藏。

## 5. 皮肤库与换肤

- **皮肤库**：本机的一个文件夹（应用数据目录下）加一份索引。皮肤按内容的 SHA-256 命名，所以重复添加同一张不会存两份；只收 PNG、64×64 或 64×32、64 KiB 以内（检查文件头，不先解码）。每张有名字、经典 / 纤细（Alex）两种模型、来源。
- **加皮肤**：从文件、从玩家名（Mojang 官方的皮肤）、从下载页。
- **换皮肤**：选一张皮肤，点「换上」。如果有不止一个**正版**账户，弹出模态框，里面是账户卡片（头像用现在画的那个头），**可多选**，对选中的每个账户各换一次；只有一个正版账户就直接换。离线账户没有皮肤可换，不列出。换肤要用 Minecraft 的访问令牌，只在后端里用，不过 IPC。**已做**（`docs/modules/skin-library.md`「换上」）。与上面不同的一处：只有一个正版账户时也弹框，账户默认选中，点一次确认——换肤立刻生效，Mojang 对更换频率有限制，误点一下的代价比多点一次大。要改成直接换，改 `WearSkinDialog` 即可。
- **下载页的皮肤**（2026-10-31 负责人定，依据调研）：只自动取**有官方接口**的来源——Mojang 官方链路（玩家名 → UUID → 会话服务器 → `textures.minecraft.net`，贴图哈希是 PNG 的 **SHA-256**，下载后校验）、LittleSkin / Ely.by（authlib-injector 体系，文档公开）。NameMC、Skindex、Planet Minecraft 没有官方接口，条款也没核实，**不自动抓取**，只放「在浏览器里打开」的按钮，用户自己保存 PNG 后加进皮肤库（走第 5 条的「从文件加」，受同样的检查）。下载的去向是「皮肤库」或「选定的文件夹」，和第 4 条的文件夹规则一样。Crafatar、Minotar、mc-heads 这类渲染服务是镜像，不是信任根，不作为皮肤文件的来源。

**已定（2026-11-01，负责人）**：皮肤库是左边导航里的独立栏目，放在「下载」后面，不再是账户页里的标签。皮肤是本机的一份收藏，不属于某个账户，「换上」时才选账户。

## 6. 第三方账户登录

Yggdrasil 协议的外置登录服务器（LittleSkin、Ely.by、Blessing Skin 系）。登录、刷新、移除和启动已做，见 [yggdrasil.md](../modules/yggdrasil.md)。密码不存、不进日志、不过 IPC，只存令牌（系统凭据库）；服务器地址只允许 HTTPS 且解析到公网地址。启动时用 authlib-injector（`-javaagent`），JAR 只信任官方 HTTPS 清单里的 SHA-256，下载后校验并缓存，每次启动重新校验。

待定：能不能通过这些服务器的接口换肤。

## 7. i18n

方案（待负责人确认）：翻译放在共享的 JSON 目录里（`locales/<语言>.json`，扁平键，`{名字}` 占位，复数用 `Intl.PluralRules` 的类别），**当前的 React 界面和之后的原生界面读同一份**，所以换原生界面时翻译不用重做。Rust 一侧有一个小 crate 读同一批文件，测试保证各语言的键和占位符一一对应。先做简体中文（现在的文案）和英文；缺的键退回简体中文。后端现在返回英文句子，之后要给错误换成稳定的错误码，由界面翻译。

待定：除了中、英还要哪些语言；语言跟系统还是在设置里选；后端错误码的做法。

## 8. 拖放安装

分类、短期单次令牌、流式复制、实例及多世界选择和逐项报告已实现，详见 [拖放安装](../modules/drop-install.md)。目前没有原生拖放入口：Tauri 2 的默认处理器把路径发给网页，已禁用。`NativeDropListener` 是占位接口；待通过 Tauri 上游提供仅 Rust 可见的回调，或实现 Windows 原生拖放钩子，再把文件路径交给后端并只向网页发送令牌与文件信息。不要启用 `tauri://drag-drop`。

浏览器开发预览可用 `?mock=drop` 模拟事件；Rust 测试直接调用 `stage`，无需把路径传过 IPC。桌面开发构建可用 `simulate_dev_drop` 命令（配合 `AETHER_DEV_DROP` 环境变量）走完整流程，见 [拖放安装](../modules/drop-install.md)。

## 顺序

1. 现在能并行、互不依赖的：世界管理（3）、i18n 的基础（7）、各项调研。
2. 内容下载（4）先做 Modrinth：客户端、安装目标、哈希校验，再做下载页的界面；CurseForge 等调研和 key 的决定。
3. 皮肤库（5）依赖调研；换肤接口和账户多选弹窗可以先做，来源最后。
4. 第三方登录（6）依赖调研，改动最大（`aether-auth`、启动参数、账户页）。
5. 原生界面工具包的决定不挡着以上任何一项：这些功能的后端都在 `aether-app`，两个界面都能用。
