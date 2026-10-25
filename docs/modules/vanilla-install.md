# Vanilla 安装流水线：版本目录阶段

本阶段接通 Mojang Java Edition 官方 `version_manifest_v2.json`，不是游戏文件安装器。

## 已实现

- `aether-manifest::catalog` 纯解析：检查 JSON、版本 ID、重复条目、官方 HTTPS 元数据 URL、SHA-1 格式及最新版本引用，限制输入大小为 2 MiB、最多 4096 条；未知元数据字段可兼容。
- Tauri `list_minecraft_versions` 只读命令：固定从 `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` 读取；通过现有生产 HTTP 传输层，12 秒超时，2 MiB 读取上限；网络操作置于 `spawn_blocking`，前端不能传自定义 URL。
- 创建实例对话框：桌面环境打开时获取官方正式版和快照版目录；失败回退到本地常见版本和手动输入，并明确说明**创建实例不会自动下载游戏文件**。

## 下一阶段（尚未实现）

1. 下载所选版本清单，验证目录提供的 SHA-1，并安全发布到受管理的 `versions/<id>/<id>.json`。
2. 用已有的 `DownloadPlan` / `DownloadEngine` 安装并核验客户端 JAR、依规则选择的 Libraries、Asset Index、Asset Objects 和 Natives；下载目标只接受受管理的相对路径。
3. 先完成可信资源与 Java 前置检查，再暴露受控的安装/取消/重试 IPC 和真实进度 UI；安装成功才将实例标记为可启动。
4. 加入伪造清单、哈希错误、路径越界、网络中断和取消后的端到端回归测试。下载源不是信任根，哈希必须由 Mojang 清单提供。

目前不支持一键安装 Vanilla，也没有把离线实例创建包装成安装成功。
