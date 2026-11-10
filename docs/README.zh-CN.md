# Logic Architecture Studio

基于 Tauri 2 / Rust 的 EDA 初始实现。GUI 和 stdio MCP 共用同一个 Rust 设计服务。界面支持英语、日语和简体中文。

运行 `npm ci`，然后运行 `npm run desktop`。浏览器开发模式需要分别运行 `npm run core` 和 `npm run dev`。可通过 `LAS_WORKSPACE` 指定工作区目录。目前以 macOS 为开发目标，IPC 使用 Unix socket。

已实现：1～64 位组合电路编辑、原子补丁、修订冲突检查、持久化撤销/重做、Verilog 生成、信号求值、独立 ALU 规范的 320 向量测试、隔离候选比较、面板停靠/浮动代码，以及 UNGE 风格节点和原生 GPU 渲染。桌面主编辑器使用 Rust 管理的原生 GPU 渲染和交互，WebView 面板支持停靠和浮动；浏览器开发模式保留 SVG。不通过 JavaScript 传输图像帧。

默认 ALU 规范：`op=0: ADD`、`1: AND`、`2/3: XOR`。内置实验有意引入 ADD→SUB 和 XOR→OR 变异，以验证测试能够拒绝错误候选。这是行为测试，不是形式证明或实测架构优化。

仅在检测到已安装 Yosys 时调用综合。不会虚构面积、延迟或功耗。内置 LLM、MCP 客户端、Verilator、形式验证、OpenROAD 和制造流程尚未实现。

请参阅[路线图](../ROADMAP.md)、[架构](ARCHITECTURE.md)、[MCP 配置](MCP.md)和[UNGE 许可声明](../THIRD_PARTY_NOTICES.md)。构建和测试命令见主 README。GPU 与 socket 集成测试需要显式添加 `-- --ignored`。

原生端与 WebView 仅同步变化的实体。移动确认会追加并 fsync `.logic/workspace.wal`，定期合并到 `.logic/workspace.json`。重启后重放移动及撤销/重做历史；文件备份需要同时保留两个文件。导出当前电路请使用应用的导出功能。参见 [2,000 节点实测](NATIVE-PERFORMANCE.md)。

当前电路、历史和候选电路共享不可变 Arc 快照；检查点采用缓冲写入，并检查 flush 和 fsync。参见 [140 次连续移动与检查点实测](HISTORY-PERFORMANCE.md)。

移动通过 ID 索引更新，仅复制和比较变化的 32 实体数据块，并保留未变化的数据；提交后跳过旧状态帧。参见[普通移动延迟实测](SPARSE-MOVE-PERFORMANCE.md)。

启动恢复界面可选择有效的当前/上一个检查点，或仅重置损坏的面板布局。恢复前先备份原文件；电路恢复不会重放之后的移动日志，布局重置则保留日志。参见[恢复行为与验证](RECOVERY.md)。

macOS 文件夹按钮可切换工作区并记住选择。记住的文件夹不存在时会显示选择界面，不会创建替代数据。参见[工作区选择](WORKSPACES.md)。

可通过工具链面板或MCP `verify.rtl`，在Icarus中运行生成的RTL，并与Rust及独立预期值比较。详见 [RTL验证](RTL-VERIFICATION.md)。
