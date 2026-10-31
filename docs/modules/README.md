# 模块文档

`docs/architecture.md` 讲的是整体分层：哪些 crate 在上、哪些在下、谁调用谁。本目录讲的是另一件事：单个模块内部的职能分界线，以及它明确不做什么。前者回答「在哪一层」，后者回答「到哪儿为止」。

四篇都面向实现者：改动对应 crate 之前先读一节，比从头读代码快。

## 模块文档索引

| 文档 | 覆盖范围（crate） | 一句话说明 |
| --- | --- | --- |
| [账户与凭据](auth.md)                 | `aether-auth`                   | 微软设备码登录的步骤、令牌链各段的失败含义、凭据文件存了什么          |
| [下载引擎](download-engine.md)        | `aether-download::engine`       | 传输 trait 的边界、`.part` 与原子 rename 的写盘协议、续传与重试的取舍 |
| [模组加载器安装](modloader.md) | `aether-modloader`、`src-tauri` | 装 Forge / NeoForge / Fabric 的流程、下载怎么校验、哪些还没做 |
| [微软账户登录](microsoft-login.md) | `aether-auth`、`src-tauri` | 设备码登录怎么走、令牌放在哪、怎么申请 Azure 应用和 Mojang 审核 |
| [实例生命周期](instance-lifecycle.md) | `aether-storage`、`aether-core` | 实例目录长什么样、写入怎么保证不残缺、导入为什么只补缺失文件          |
| [实例管理](instance-management.md) | `aether-storage`、`src-tauri` | 管理页能做什么、前端为什么不传路径、日志怎么脱敏、哪些还没做 |
| [内容元数据](content-metadata.md) | `aether-content`、`aether-app` | 模组、资源包、光影怎么用文件哈希到 Modrinth 认出名字和图标、什么会离开本机、答案怎么当不可信输入处理 |
| [投影原理图](schematics.md) | `aether-schematic`、`aether-storage`、`src-tauri` | 三种格式的读写和互转、换版本能做和不能做什么、3D 预览的帧预算 |
| [崩溃分析边界](crash-analysis.md)     | `aether-diagnose`               | 六个内部模块的分工、不可信日志的处理、为什么第一层脱敏不做高熵替换    |

四篇的写法一致：先说这个模块负责什么，再说它不负责什么，最后列出改它之前要知道的约束。如果只想找某个函数的行为，直接看代码，文档不重复抄签名。

## 阅读前提

读之前先确认三件事，否则容易把文档里的方向当成代码里的现状。看的时候先看分支名和标注，再看正文；正文里没有标注的部分，默认按目标形态理解。

- 这几篇描述的是分支 `feat/backend-foundations-expansion` 的目标形态，不是当前代码的快照。代码还没落地的地方，文档写的是方向，不是事实。
- 文中标了「待实现确认」的段落以代码为准。读的时候如果发现文档与代码冲突，先相信代码，再顺手改文档。
- 模块边界的变化不是静默的。模块职责的调整记进 `CHANGELOG.md` 的 `### Changed`；涉及用户可见的安全边界时另加 `### Security`。

以上三条不是免责声明，而是读法：第一条决定你该不该信细节，第二条决定冲突时听谁的，第三条决定你去哪里找边界变动的记录。

## 相关文档

- [`docs/design/backend-foundations-expansion.md`](../design/backend-foundations-expansion.md)：本分支的设计总纲，写清 PR #12 之后这轮扩展拆成了哪几块、按什么顺序落地。
- [`docs/design/crash-analysis.md`](../design/crash-analysis.md)：崩溃分析的设计篇，讲产品流程、外发与脱敏的评估；crate 内部的实现边界不在这里，见 [崩溃分析边界](crash-analysis.md)。

## 什么时候该新写一篇模块文档

判断标准要能被别人复现，所以写成可判定的门槛。下面四条满足任意两条，就值得单开一篇：

1. 该 crate 有独立的失败模式或状态机，一句话讲不完。比如有多个终态、有重试与放弃的分叉，光看函数名猜不出运行时行为。
2. 该 crate 的边界会被反复误解，具体表现是有人问过「这算不算这个模块的事」。同一个疑问出现过两次，就说明这条边界值得写下来。
3. 必须写清「不做什么」才能防止误用。也就是说，代码本身没有拒绝某种用法的机制，只能靠文档约定。
4. `docs/architecture.md` 里对应的段落已经超过 15 行且还在增长，说明它已经超出了「分层概览」的容量，该搬走了。

反过来，如果只是给一个函数或一个配置项写说明，不要开新文档：写在代码注释里，或者补进 `docs/architecture.md` 的对应段落。文档数量本身不是目标，能在一个地方查完的事情不要拆成两处；拆开之后两处都会慢慢过期。
