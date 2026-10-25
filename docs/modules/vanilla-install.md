# Vanilla 安装流水线：版本目录与可信 JSON 缓存阶段

本阶段接通 Mojang Java Edition 官方 `version_manifest_v2.json`，不是游戏文件安装器。

## 已实现

- `aether-manifest::catalog` 纯解析：检查 JSON、版本 ID、重复条目、官方 HTTPS 元数据 URL、SHA-1 格式及最新版本引用，限制输入大小为 2 MiB、最多 4096 条；未知元数据字段可兼容。
- Tauri `list_minecraft_versions` 只读命令：固定从 `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` 读取；通过现有生产 HTTP 传输层，12 秒超时，2 MiB 读取上限；网络操作置于 `spawn_blocking`，前端不能传自定义 URL。
- 创建实例对话框：桌面环境打开时获取官方正式版和快照版目录；失败回退到本地常见版本和手动输入，并明确说明**创建实例不会自动下载游戏文件**。
- `cache_vanilla_version_manifest(version)`：IPC 只接受官方版本 ID，后端重新读取可信目录并绑定 URL 与 SHA-1；单个版本 JSON 最多 8 MiB，读取后先核验 SHA-1 和 JSON 中的版本 ID / 官方客户端元数据。
- 只有上述校验全部通过才在 `app_data_dir/game/versions/<id>/<id>.json` 内通过 `create_new` 临时文件与 rename 发布。已存在的错误/损坏缓存不会被自动覆盖；符号链接路径被拒绝。缓存失败不产生正式版本文件，也不改变实例状态。该目录由本启动器管理，不改写现有 `.minecraft`。
- `aether-launch::build_install_plan(manifest, client_jar_version, environment)`：从已缓存、已校验的版本 JSON 生成 `DownloadPlan`，包含 `versions/<id>/<id>.jar`、按规则启用的 Libraries 的 `artifact`，以及复用 `build_native_plan` 选出的 Natives 压缩包。URL、SHA-1、大小都来自官方 JSON；缺少 `downloads.artifact` 的库（例如只给 Maven 仓库地址的加载器库）直接报错，不退回到镜像或自行拼 URL。路径越界、重复目标（Windows 下不区分大小写）、非 HTTPS 地址都会使整个计划失败。
- `install_vanilla_client_libraries(version)`：IPC 只接受版本 ID，要求该版本 JSON 已缓存；经 `DownloadEngine`（4 路并发、重试、断点续传、SHA-1 校验）把上述文件写入 `app_data_dir/game/`。成功返回已校验文件数；失败只返回「N / M 个文件失败」，不回显下载地址。创建实例对话框的「下载并安装此版本」按钮依次执行 JSON 缓存和本命令。

## 下一阶段（尚未实现）

1. 用同一套 `DownloadPlan` / `DownloadEngine` 安装并核验 Asset Index、Asset Objects，并解压 Natives（客户端 JAR、Libraries 与 Natives 压缩包的下载已完成）；下载目标只接受受管理的相对路径。
2. 先完成可信资源与 Java 前置检查，再暴露受控的安装/取消/重试 IPC 和真实进度 UI；安装成功才将实例标记为可启动。
3. 加入伪造清单、哈希错误、路径越界、网络中断和取消后的端到端回归测试。下载源不是信任根，哈希必须由 Mojang 清单提供。

现在可以在创建实例对话框安装一个 Vanilla 版本（JSON、客户端、Libraries、Assets），再创建实例，由启动面板校验并离线启动。启动使用启动器私有的 app_data_dir/game/，不再读取官方 .minecraft。尚无取消和进度 UI。离线实例创建、JSON 缓存和完整游戏安装是三个不同阶段，不会因前两者成功而将实例标记为 Ready。后续安装目录与当前仅负责启动官方已安装版本的协调器须显式衔接，不能把缓存成功当作可启动。
