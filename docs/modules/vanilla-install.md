# Vanilla 安装流水线

从 Mojang 官方目录到一个可以启动的实例：版本目录 → 版本 JSON → 客户端、依赖库、本地库压缩包 → 资源索引与对象 → 创建实例。前端只看到一个按钮，见 [`docs/design/ui-shell.md`](../design/ui-shell.md)。

## 信任模型

- 前端只传版本号和实例名，从不传 URL、路径或哈希。
- 版本目录固定从 `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` 读取；每个版本 JSON 的 URL 和 SHA-1 由这份目录给出，下载后先核验再解析。
- 之后所有文件的 URL、大小、SHA-1 都来自这份已校验的版本 JSON。镜像不是信任根，缺少官方哈希的库（例如只给 Maven 仓库地址的加载器库）直接报错。
- 资源对象的 URL 由摘要推出（`resources.download.minecraft.net/<前两位>/<摘要>`），摘要来自已校验的资源索引。
- 任何下载目标都必须是安全的相对路径；符号链接路径被拒绝。

## 各层的职责

- `aether-manifest::catalog`：纯解析目录。检查 JSON、版本 ID、重复条目、官方 HTTPS 元数据 URL、SHA-1 格式；输入限制 2 MiB、4096 条。
- `src-tauri` 的 `list_minecraft_versions`：只读命令，`spawn_blocking`，12 秒超时，不接受自定义 URL。
- `vanilla::prepare_version`：重新读取可信目录，核验单个版本 JSON（最多 8 MiB）的 SHA-1、版本 ID 和官方客户端元数据，通过 `create_new` 临时文件与 rename 发布到 `app_data_dir/game/versions/<id>/<id>.json`。已存在但损坏的缓存不会被自动覆盖。
- `aether-launch::build_install_plan(manifest, client_jar_version, environment)`：从版本 JSON 生成 `DownloadPlan`——`versions/<id>/<id>.jar`、按规则启用的库的 `artifact`、以及 `build_native_plan` 选出的本地库压缩包。路径越界、重复目标（Windows 下不区分大小写）、非 HTTPS 地址都使整个计划失败。
- `aether-download::assets::build_asset_object_plan`：从已校验的资源索引为每个不同的对象生成计划。
- `install::install_and_create`：把上面的步骤串起来，然后创建实例并校验客户端和资源，标记为可启动。

## 安装命令

`install_vanilla_version(version, name)`，一次只允许一个安装：

1. 先检查实例名（长度、重名），失败就在下载前返回。
2. 元数据 → 客户端和库 → 资源索引与对象 → 创建实例。
3. 每个阶段先扫描本地：**已存在且通过校验的文件直接跳过**，所以再次安装、断点后继续、第二个同版本实例都很快。其余文件经 `DownloadEngine`（4 路并发、重试、断点续传、SHA-1 校验）下载。
4. 进度以 `install://progress` 事件推送（阶段、文件数、字节数，约 100 ms 一次）。`cancel_vanilla_install()` 取消；已下载的文件保留。
5. 失败时只返回「N / M 个文件失败（原因 ×数量）」，原因分类为 HTTP 状态码、超时、网络、哈希不符、本地文件，不回显下载地址。

本地库在每次启动前由协调器解压并逐文件校验，安装阶段只下载压缩包。

## 实机验证

仓库里有两个默认忽略的实机测试，在 Windows 上对真实的 Mojang 服务器跑过：

```text
AETHER_LIVE_VERSION=1.21.1 AETHER_LIVE_DIR=<目录> \
  cargo test -p aether-desktop --lib live_install -- --ignored --nocapture
AETHER_LIVE_VERSION=1.21.1 AETHER_LIVE_DIR=<同一目录> \
  cargo test -p aether-desktop --lib live_launch -- --ignored --nocapture
```

第一个完整安装一个版本并重新校验全部资源；第二个创建实例、启动游戏并确认 30 秒后仍在运行。它们找到过 3 个只有真实运行才会暴露的问题（本地库的 `META-INF` 冲突、Windows 路径分隔符、`\\?\` 前缀的类路径），修复都带了回归测试。

## 未做

- `assets` 为 `legacy` / `pre-1.6` 的旧版本需要虚拟资源目录映射，且 1.13 之前的版本使用旧的启动参数格式，目前都不支持。
- 加载器（Fabric、Forge、NeoForge、Quilt）的安装；它们的库通常只给 Maven 仓库地址，需要先决定哈希从哪来。
- 对已有实例的就地修复，以及多个安装排队。
