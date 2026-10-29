# 从其他启动器导入

用户已经在 PCL2、HMCL、BakaXL、Prism Launcher / MultiMC 或一个普通 `.minecraft` 里装过游戏。导入的目的是让他们不用再下载一遍，同时不破坏启动器的安全边界。

## 设计

**导入不把别人的文件夹当作游戏目录。** 所有实例都在启动器自己的游戏目录（`app_data/game`）里，和一键安装产出的实例完全一样。来源文件夹只做一件事：当"种子"。

1. **检测与预览**（`aether_storage::import` 检测层 + `src-tauri/src/commands/import.rs`）：后端打开系统文件夹选择框或在有限范围内查找已知启动器，扫描后返回候选版本列表和一个**一次性 token**。前端从不传路径，导入时只提交 token 和用户勾选的条目 ID。
2. **规划**（`plan_folder_import` / `plan_prism_import`，只读）：对每个勾选项做校验——版本 ID 必须是安全的单段文件名，版本 JSON 必须是不超过 8 MiB 的普通文件（拒绝符号链接），能解析出基础版本号。需要加载器的条目（`inheritsFrom`、Prism 实例里有 Fabric/Forge 等组件）直接跳过，原因 `needs-loader`。规划不写任何文件。
3. **安装**（`install_and_create_with`）：对每个规划出的版本，按官方清单取回并校验版本 JSON（不在官方清单里的跳过，`not-official`），再走和一键安装同一条下载流程。下载前，每个文件先检查本地，再检查种子：**种子里同路径的文件，大小和 SHA-1 与官方 JSON 一致才会被复制**（先写 `.seed` 暂存文件，复制后再校验一次，最后改名）；不一致、不存在或任何文件系统错误都回退为正常下载。

结果是：用户已有的 jar、libraries、assets 不用重新下载，但进入游戏目录的每个字节都经过官方清单的哈希校验，来源文件夹既不会被写入，也不会被启动。

## 支持与不支持

| 情况 | 结果 |
| --- | --- |
| 官方正式版 / 快照（无加载器） | 导入 |
| 带 Fabric / Forge / Quilt / NeoForge 的版本或实例 | 跳过，`needs-loader` |
| 不在官方清单里的版本（自定义 JSON） | 跳过，`not-official` |
| 实例名与现有实例重复 | 跳过，`duplicate-name` |
| 下载或校验失败 | 跳过，`install-failed`，可重试，已下载的文件保留 |
| 用户取消 | 剩余项 `cancelled` |

mods、saves、config、资源包、光影包**不导入**。加载器支持在启动器有自己的加载器安装流程之后再做。

## 安全边界

- 前端不能传路径；token 单次有效，数量有上限。
- 来源是不可信输入：版本 JSON 有大小上限，符号链接和非普通文件被拒绝，条目数有上限（256）。
- 哈希来自官方清单，种子只是"另一个下载来源"，不是信任根。
- 错误和跳过原因只回传稳定的分类字符串，不带本机绝对路径。
- 目录选择框由后端调用（`tauri-plugin-dialog` 的 `blocking_pick_folder`），没有给前端开放 `dialog:*` 权限。

## 参考

只参考了这些启动器的目录结构与配置字段的**行为**（`versions/<id>/<id>.json`、`mmc-pack.json`、`multimc.cfg`、PCL2 的 `Setup.ini`、`.hmcl.json`），未复制它们的代码或测试数据。

## 待定

- 加载器版本的导入方式（需要启动器自己的加载器安装器）。
- 是否同时导入 mods / saves，以及它们应该落在哪个目录。
- `tauri-plugin-dialog` 作为新依赖，需要维护者确认。
