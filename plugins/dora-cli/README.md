# dora-cli（Component debug）

只读的组件调试面板。它把本机 `dora` CLI 当作子进程调用，观察 **coordinator 上正在
运行**的 dataflow：topic 列表、`topic echo` / `topic hz`、node 列表、`node info`、
param 列表与 `param get`。

- 插件 id：`dorobot.dora-cli`
- 面板 id：`page_dora_cli`
- 标题：`Component debug`
- 依赖：本机 `dora-cli`（不链接 `dora-node-api`）

## 前置条件

1. **能解析到 CLI。** 优先 `DOROBOT_DORA_BIN`，否则 `PATH` 上的 `dora`。

   ```sh
   export DOROBOT_DORA_BIN=/Users/you/.bin/dora
   /Users/you/.bin/dora --version
   ```

   面板顶部会显示实际解析到的路径和 `--version` 输出，因为不同 Dora fork 的
   CLI 参数可能不同，面板不硬编码版本。

2. **dataflow 必须跑在 coordinator 上。** 在终端里：

   ```sh
   dora up
   dora start path/to/dataflow.yaml
   dora list
   ```

   `dora run` 会启动隔离的 coordinator，外部 CLI 查询不到它；本面板也查不到。
   这是使用本面板最常见的空态原因。

3. **topic echo / hz 需要 dataflow 开启 debug inspection。** 在 dataflow YAML 中：

   ```yaml
   _unstable_debug:
     enable_debug_inspection: true
   ```

## 使用步骤

1. 打开侧栏 **Component debug**。
2. 顶部核对三行信息：CLI 路径、`--version`、coordinator 状态（`dora list` 的结果）。
   若显示 `not reachable`，按上文启动 `dora up` 与 `dora start`，点
   **Refresh coordinator**。
3. **Topics**：点 **Refresh** 读取 `dora topic list`；点击一条 topic（形如
   `node_id/output_id`），再点 **Echo** 或 **Hz (60 s)**。输出出现在中间卡片。
   - 同一时刻只保留一个流；启动新流会先杀掉旧流。
   - **Stop** 立即取消并杀掉 CLI 子进程。
4. **Nodes**：点 **Refresh** 读取 `dora node list`；点击一个 node，会执行
   `dora node info <NODE>` 并自动刷新该 node 的 `dora param list <NODE>`。
5. **Params**：点击一个 param，执行 `dora param get <NODE> <KEY>`，结果显示在下方。
6. 底部命令行日志会记录最近命令、退出码与耗时；流结束时也会记录退出码与持续时间。

## 实际命令（已用本机 `dora-cli 1.0.0-rc1 --help` 核对）

| 面板动作 | 实际 argv |
|---|---|
| 连接 CLI | `dora --version` |
| coordinator 状态 | `dora list` |
| 刷新 Topics | `dora topic list` |
| 刷新 Nodes | `dora node list` |
| 查看 node | `dora node info <NODE>` |
| 刷新 params | `dora param list <NODE>` |
| 查看 param | `dora param get <NODE> <KEY>` |
| Echo | `dora topic echo <NODE>/<OUTPUT>` |
| Hz | `dora topic hz <NODE>/<OUTPUT> --window 10 --duration 60` |

注意：

- `param list` / `param get` 的 node 是**位置参数**，不是 `--node <node>`。
- `topic hz` 在 stdout 不是终端时不启动 TUI，**必须**带 `--duration`；本面板固定
  60 秒，期间可用 **Stop** 提前取消。
- 所有命令都用 argv 数组传给 `Command`，不经过 shell，因此含空格/特殊字符的名字
  不会被拆分。
- 面板未暴露 `-d/--dataflow <UUID_OR_NAME>`；coordinator 上同时跑多个 dataflow 时，
  现有命令可能无法区分，属于已知限制。

## 查看结果与空态

- 输出区最多保留 5000 行（有界环形缓冲），界面显示最新 400 行，单行 600 字符。
- topic/node/param 列表从 CLI 的表格或 JSON 输出中尽力解析；解析不到时会直接显示
  原始输出，避免“看起来为空”。
- 未启动 coordinator、`dora run` 隔离模式、CLI 不可执行、命令超时（一次性命令
  5 秒）都会在面板中显示原因，而不是静默失败。

## 只读边界

本面板只实现读取命令。**没有** `param set`、`topic pub`、`node restart`，也不启动/
停止 dataflow，不接 `dora-node-api`，不写任何 dataflow 文件。

## 故障排查

| 现象 | 处理 |
|---|---|
| `CLI: not found` | 设置 `DOROBOT_DORA_BIN` 指向可执行文件，或把 `dora-cli` 加入 `PATH` |
| `cannot connect to coordinator` | 先 `dora up`，再用 `dora start` 启动 dataflow |
| 有 dataflow 但 topic 列表为空 | 确认 dataflow 开了 `_unstable_debug.enable_debug_inspection` |
| Hz 没有持续输出 | 非 TTY 模式只在 60 秒后打印最终统计；这是 CLI 行为 |
| 列表解析为空但命令 exit 0 | 查看卡片里保留的原始输出；表格格式可能随 CLI 版本变化 |
