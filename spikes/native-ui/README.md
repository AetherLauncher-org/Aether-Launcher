# 原生界面工具包对比

`docs/design/native-ui.md` 第 2 步：在 iced 和 egui 上各做同一个小范围，比输入法、外观、帧时间、内存、体积、许可证。**这是一次性的对比代码，不是启动器的一部分**：自己的 workspace，不被根 `Cargo.toml` 收录，提交检查不编译它，启动器里没有任何东西依赖它。结论和数字在 `docs/design/native-ui.md` 的「对比结果」里。

```text
common/   两边共用：数据（经 aether-app 列实例、改名）、表单逻辑、方块网格和 WGSL、帧时间量法
iced/     iced 0.14：shader 控件，自己的渲染通道和深度缓冲
egui/     egui 0.37：画到自己的纹理，再当图片贴出来
```

## 运行

```powershell
cd spikes\native-ui
cargo build --release -p spike-iced -p spike-egui   # 冷编译各约 7 分钟，内存紧时加 CARGO_BUILD_JOBS=2
.\target\release\spike-iced.exe                      # 自己用：试输入法、点实例、改名
.\target\release\spike-egui.exe
```

数据放在系统临时目录的 `aether-native-ui-spike/`（第一次运行会建几个示例实例）。

## 测量

```powershell
powershell -File bench.ps1          # 每个工具包 × {转动, 静止} × 2 次，每次 14 秒，结果写进 bench-results.txt
powershell -File screenshot.ps1 spike-iced    # 只截窗口本身，存到 screenshots\
```

`--bench animate|idle <秒>` 也可以直接给可执行文件。前 4 秒是预热，`steady` 开头的数字是之后的部分，不含设备、着色器和字体的初始化。

## 输入法（需要人来试）

自动测不了。两个窗口都有一个中文输入框（选实例后里面是实例名）：用拼音和五笔输入，看候选窗口是否跟着光标、能不能选词、回车是提交输入还是确认候选、和快捷键（Ctrl+A、Ctrl+Z）是否冲突，再试一次日文输入法。结果写回 `docs/design/native-ui.md`。
