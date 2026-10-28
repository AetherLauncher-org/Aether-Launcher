## 概述

让玩家把已经在用的启动器里的版本一键搬进 Aether：选一个启动器（或直接挑一个 `.minecraft` 文件夹），勾要哪些版本，导入后就能在实例列表里启动。

只复制**版本元数据**，游戏本体、库、资源仍由现有的原版安装器下载，并且照旧「官方源优先、BMCLAPI 兜底」。所以导入不依赖任何别的启动器的运行时，也不改动玩家原来的安装。

## 支持的范围

| 来源 | 怎么找 | 说明 |
| --- | --- | --- |
| 直接选文件夹 | 后端打开原生文件夹选择框 | 任何 `.minecraft` 都行 |
| HMCL | 扫描发现 + 手动指定 | 读 `versions/<版本>/` | 
| PCL2 | 读 `PCL/Setup.ini` 的 `LaunchFolderSelect`（支持 `$` 相对启动器目录） | 扫描找不到时允许手动指定 |
| BakaXL | 按启动器标记识别，游戏目录取可执行文件旁的 `.minecraft` | |
| Prism Launcher / MultiMC | 读 `mmc-pack.json` 与 `instance.cfg` | 每个实例建一个 Aether 实例，loader 组件只报告不安装 |

一次「一键导入」会在受控范围内把这几种都探一遍，识别到哪个就在对话框里标出来。

## 行为约定

- **前端从不传路径。** 选择文件夹由后端用 `tauri-plugin-dialog` 打开，扫描结果用一次性 token 引用；`import_versions` 只接受 token 与版本名。token 用掉即失效，过期 token 会被拒绝，而不是悄悄从别的目录导入。
- **只填缺失的文件。** 如果目标已经存在同名版本，不覆盖、也不重复建实例，界面上说「已经导入过」而不是报错。
- **`inheritsFrom` 一起复制。** 只复制子版本会导致导入后无法启动，所以父链上的版本也一并写进受管游戏目录。
- **导入后补齐运行环境。** 客户端 jar、libraries、natives、assets 走现有的 `install_vanilla_client_libraries`，失败时按既有策略回退镜像。
- **不改别人的安装。** 所有写入都发生在 Aether 自己的游戏目录，绝不修改 HMCL/PCL2 的原文件。

## 主要改动

**后端**

- `crates/aether-storage/src/import/`：`mod.rs`（来源识别与候选计划）、`execute.rs`（把选中的版本写进受管目录）、`pcl2.rs` / `prism.rs` / `bakaxl.rs`（各家解析）、`fallback.rs`（找不到时的兜底）
- `src-tauri/src/commands/import.rs`：`pick_import_folder`、`detect_launcher_installations`、`pick_launcher_folder`、`import_versions`
- `src-tauri/capabilities/default.json`：只放开对话框权限

**前端**

- `src/components/ImportInstancesDialog.tsx`：来源列表、版本多选、导入进度与逐版本结果
- `src/ipc/import.ts`、`src/lib/import.ts`：类型与纯函数（版本去重、默认勾选、结果汇总）

## 测试

- `cargo test -p aether-storage`：来源识别、PCL2 `Setup.ini` 解析（含 `$` 展开）、Prism `mmc-pack.json`、继承链复制、只填缺失文件、重名不误报
- `cargo test -p aether-desktop --lib`：命令层对 token 复用、越界版本名、空选择的拒绝
- `src/lib/import.test.ts`、`src/components/ImportInstancesDialog.test.tsx`：默认勾选、全部失败与部分失败的结果文案

## 还没做 / 已知限制

- Forge/Fabric/Quilt/NeoForge 的 loader **只报告，不安装**。导入后实例能启动，但不会自动把 loader 装上；这一步留给后续的 loader 安装 PR。
- 一键扫描只覆盖上面五种来源；其它启动器需要手动选文件夹。
- 导入进度是逐版本汇报，没有总体百分比。

## 怎么验证

1. 装一个 HMCL 或 PCL2，里面至少两个版本（其中一个带 `inheritsFrom`）
2. 实例页 → 导入 → 一键扫描，勾两个版本 → 导入
3. 确认实例列表出现对应条目，且 `inheritsFrom` 的父版本也在受管游戏目录里
4. 分别用官方源与断网（强制走镜像）各启动一次

## Checklist

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test -p aether-storage`
- [x] `cargo test -p aether-desktop --lib`
- [x] `npm run check`
- [x] `docs/architecture.md` 已同步导入流程与边界
- [x] `CHANGELOG.md` 已记录
- [x] 未引入 GPL/LGPL 代码，只参考行为
