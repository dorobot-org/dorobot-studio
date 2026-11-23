# dviz 插件 P1（插件化）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标：** 把 dviz 做成 DoRobot Studio 的插件。插件用 Zenoh 接收 dviz 通用协议，写进它拉起的 Rerun 0.28 窗口，并在 Studio 里提供连接、显示列表、日志、节点 / 数据流图面板。用一个不依赖 Dora 的合成发布器验证。

**架构：**
- 协议代码搬进 `crates/dviz-protocol`。
- 插件 `plugins/dviz` 分四块：
  - 进程级会话 `DvizSession`：Zenoh 接收线程、路由、快照；
  - `Sink`：写 Rerun；特性关闭时用空实现；
  - Studio 写法的面板；
  - 自绘的数据流图。
- 合成发布器是插件 crate 的 example。

**技术栈：** Rust（CI 1.95）、Makepad（OctoSense `44d8220f`，`script_eval!`）、Zenoh `=1.10.2`、Rerun `0.28`、serde_json。

**设计：** `docs/superpowers/specs/2026-11-22-dviz-nav-plugin-design.md`（2026-11-22 批准）。本计划只覆盖其中的 **P1**；P2 另写计划。

**和设计的偏离（计划时的决定）：** 设计"仓库内的放置"一节写了"`crates/dviz-transform` 原样搬入"。P1 里变换由 Rerun 自己管，没有代码用到 TF 缓冲，所以**这次不搬**，等用得上时再搬（YAGNI）。代价：真用到时再花约半天搬。

## 全局约束

- **分支和提交：** 分支 `feat/dviz-plugin`，只在本地，不推送。提交身份 `bobdingai <dingxinmin@gmail.com>`，结尾 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`。
- **构建：**
  - **这台 Orin 上**链接 `dorobot-flex` 缺 `-lpulse`、`-lXcursor`、`-lxkbcommon` 的开发版符号链接，又不能用 apt / dpkg 装 `-dev` 包。所以所有 cargo 命令前都加环境变量 `LIBRARY_PATH=$HOME/nvme/dora-lab/linkshim`：这个目录里是指向系统运行时库（`.so.0` / `.so.1`）的软链接，2026-11-22 建好。它不改 rustc 参数，不会触发重编；其他机器不需要。
  - 一律 `--locked`。新增成员或依赖时，先不带 `--locked` 跑一次 `cargo build -p <crate>` 更新 `Cargo.lock`，再提交 `Cargo.lock`。
  - 不引入 Rust 1.95 之后的特性。
- **版本：** Makepad 用工作区固定的修订；Zenoh `=1.10.2`，`default-features = false, features = ["transport_tcp"]`，和 `dorobot-studio-core` 相同；Rerun 用工作区的 `rerun`（0.28，`default-features = false, features = ["sdk"]`）。
- **插件标识：** 插件 id `dorobot.dviz`，标题 `3D visualization`；面板 `dviz_view`，标题 `Visualization`；`api_version: API_VERSION`。
- **许可证：** `crates/dviz-protocol` 和 `plugins/dviz` 写 `license = "Apache-2.0"`，因为含从 dviz 搬来的代码。仓库根加 `NOTICE`，写明来自 dviz（dorobot-org/dviz，提交 `5276598`，Apache-2.0）。搬来的文件保留原有头注释。
- **Rerun 特性开关：**
  - 写 Rerun 的代码只在插件特性 `rerun` 下编译；`dorobot-flex` 的 `default` 特性加上 `rerun`。
  - 所以 CI 的 `--no-default-features` 不编 Rerun。
  - Rerun 0.28 查看器是使用前提：`cargo install rerun-cli --version 0.28.x --locked`。
- **Zenoh 话题：** 话题前缀默认 `dviz`，订阅 `<prefix>/**`；Rerun 实体路径 = 话题去掉 `<prefix>/` 前缀，和 dviz 相同。
- **测试命令：**
  - 全仓：`cargo test --locked --workspace --no-default-features`
  - 插件：`cargo test --locked -p dorobot-plugin-dviz`、`cargo test --locked -p dorobot-plugin-dviz --features rerun`
  - 宿主：`cargo test --locked -p dorobot-flex --no-default-features --test plugins`

## Review Focus

1. **Zenoh 地址不可达或没网：** 连接应在约 2 s 内报出可见的错误，界面线程不阻塞、不卡死。→ Task 3 `connect_to_unreachable_endpoint_reports_error_without_blocking`。
2. **点云来得比 Rerun 写得快：** 队列有界，满了丢最旧的，丢弃有计数，内存不涨。→ Task 3 `burst_beyond_capacity_drops_oldest_and_counts`。
3. **坏消息：** JSON 坏、二进制比 `count×12` 短、未知类型，都只计数、丢弃，不 panic。→ Task 1 `parse_points_rejects_short_binary`、Task 3 `malformed_and_unknown_messages_are_counted_not_fatal`。
4. **没装 `rerun` 或版本不对：** 点"打开 3D 窗口"给出明确的错误；日志、节点照常工作。→ Task 4 `spawn_without_rerun_binary_returns_clear_error`、Task 5 `open_viewer_failure_is_shown_and_session_keeps_running`。
5. **反复连接 / 断开、在面板之间切换：** 不会出现第二个会话或接收线程；`on_disable` 不断开连接。→ Task 3 `reconnect_replaces_the_single_receiver`、Task 5 `disable_keeps_the_session_connected`。

---

### Task 1：`crates/dviz-protocol`（搬入通用协议）

**Files:**
- Create：
  - `crates/dviz-protocol/Cargo.toml`
  - `crates/dviz-protocol/src/lib.rs`：搬自 dviz `dviz-core/src/zenoh_protocol.rs`，原测试随之搬入
  - `NOTICE`
- Modify：根 `Cargo.toml`，`members` 加 `crates/dviz-protocol`，`[workspace.dependencies]` 加 `dviz-protocol = { path = "crates/dviz-protocol" }`
- Test：`crates/dviz-protocol/src/lib.rs` 里的 `#[cfg(test)]`

**Interfaces：**
- **原样保留**（名字不改）：`MvizMessage`、`Points3DData`、`Boxes3DData`、`Arrows3DData`、`LineStrips3DData`、`Transform3DData`、`TextData`、`ScalarData`、`LogLevel`、`LogEntry`、`LogData`、`NodeDefinition`、`GraphNodeStatus`、`GraphNode`、`GraphEdge`、`GraphUpdate`、`formats::*` 常量，以及：
  - `parse_message(bytes: &[u8]) -> Option<(MvizMessage, Option<&[u8]>)>`
  - `serialize_message(header: &MvizMessage, binary: Option<&[u8]>) -> Vec<u8>`
- **新增：** `parse_points_xyz_f32(binary: &[u8], count: u32) -> Option<Vec<[f32; 3]>>`
  - 来自 dviz-shell `zenoh_receiver.rs:333`；
  - 改为在 `binary.len() < count as usize * 12` 时返回 `None`，原来会越界或截断。

- [ ] **Step 1：基线。** 在改动之前跑 `cargo test --locked --workspace --no-default-features`，记录通过数。基线不过就停下，把原因记进台账。
- [ ] **Step 2：搬代码，写失败的测试。** 搬入文件（保留原测试），新增：
  - `round_trip_with_binary`：带 24 字节载荷的 `points3d` 消息，`serialize` 再 `parse`，头字段和载荷相等。
  - `round_trip_json_only`：没有载荷的 `log` 消息，`parse` 得到的载荷是 `None`。
  - `parse_rejects_garbage`：`parse_message(b"\xff\xfe")` 和 `parse_message(b"{not json")` 都是 `None`。
  - `parse_points_ok`：两个点的 24 字节，`count` 2，得到 `[[1,2,3],[4,5,6]]`。
  - `parse_points_rejects_short_binary`：20 字节、`count` 2，得到 `None`。
- [ ] **Step 3：确认失败。** `cargo test -p dviz-protocol` 中 `parse_points_*` 应编译失败或断言失败（函数不存在）。
- [ ] **Step 4：实现** `parse_points_xyz_f32`，小端 f32。
- [ ] **Step 5：确认通过。** `cargo build -p dviz-protocol` 更新 `Cargo.lock`；`cargo test --locked -p dviz-protocol` 全部通过。
- [ ] **Step 6：提交**（`feat(dviz-protocol): move the dviz universal Zenoh protocol into Studio (Apache-2.0)`）。

### Task 2：插件骨架和注册

**Files:**
- Create：
  - `plugins/dviz/Cargo.toml`：包名 `dorobot-plugin-dviz`。依赖 `dorobot-studio-core`（`default-features = false`）、`dorobot-studio-plugin`、`dorobot-ux`、`dviz-protocol`、`zenoh`（按全局约束的版本）、`serde_json`；可选 `rerun`（workspace）。特性 `rerun = ["dep:rerun"]`。
  - `plugins/dviz/src/lib.rs`：`DvizPlugin`，以及一个只有标题的占位面板 `DvizViewPanel`（Task 5 充实）。
- Modify：
  - 根 `Cargo.toml` 的 `members` 加 `plugins/dviz`；
  - `dorobot-flex/Cargo.toml`：依赖 `dorobot-plugin-dviz = { path = "../plugins/dviz" }`；`[features]` 加 `rerun = ["dorobot-plugin-dviz/rerun"]`，`default = ["video", "rerun"]`；
  - `dorobot-flex/src/plugins.rs`：注册 `dorobot_plugin_dviz::DvizPlugin`，放在最后。
- Test：`dorobot-flex/tests/plugins.rs`

**Interfaces：**
- Produces：`pub struct DvizPlugin;`，实现 `StudioPlugin`：
  - `descriptor()`：id `dorobot.dviz`、标题 `3D visualization`、一个面板 `{id: "dviz_view", title: "Visualization", legacy_screen: None}`；
  - `create("dviz_view", vm)` 返回面板，其他面板名返回 `Err`；
  - `can_disable` 一律允许。

- [ ] **Step 1：改测试（先失败）。**
  - `toolbox_has_one_entry_per_plugin_instead_of_one_per_panel`：`children.len()` 改为 4，加 `assert_eq!(children[3].1.text(), "3D visualization");`。
  - 新增 `dviz_panel_is_registered_and_creates_independent_views`：`registry.panel_owner("dviz_view") == Some("dorobot.dviz")`；两次 `create_instance("dviz_view", vm)` 得到不同的 `widget_uid`。
- [ ] **Step 2：确认失败。** `cargo test -p dorobot-flex --no-default-features --test plugins`：上面两条失败（3 ≠ 4、面板不存在）。
- [ ] **Step 3：实现**骨架和注册；占位面板用 `script_eval!` 生成一个带 `Label{text: "Visualization"}` 的 `View`。
- [ ] **Step 4：确认通过。** 先 `cargo build -p dorobot-flex --no-default-features` 更新 `Cargo.lock`，再 `cargo test --locked -p dorobot-flex --no-default-features --test plugins` 全部通过，包括 `every_builtin_factory_and_toolbox_has_no_script_errors`。
- [ ] **Step 5：提交**（`feat(plugins/dviz): register the dviz plugin skeleton`）。

### Task 3：进程级会话 `DvizSession`（Zenoh 接收、路由、快照）

**Files:**
- Create：`plugins/dviz/src/session.rs`、`plugins/dviz/src/route.rs`、`plugins/dviz/tests/loopback.rs`
- Test：`route.rs` 里的 `#[cfg(test)]`，以及 `tests/loopback.rs`

**Interfaces：**
- `pub trait Sink: Send { fn log(&mut self, entity: &str, msg: &MvizMessage, binary: Option<&[u8]>) -> Result<(), String>; }`
  - 定义在 `route.rs`；
  - 同文件提供 `pub struct CountingSink { pub calls: Vec<(String, String)> }`，记录 `(entity, msg_type)`，测试和未开 `rerun` 特性时用。
- `pub struct SessionState`：
  - `logs: VecDeque<LogEntry>`，上限 `LOG_CAP = 500`；
  - `nodes: Vec<GraphNode>`、`edges: Vec<GraphEdge>`；
  - `entities: BTreeMap<String, EntityInfo { enabled: bool, count: u64, msg_type: String }>`；
  - `dropped: u64`、`malformed: u64`。
- `pub fn route(state: &mut SessionState, sink: &mut dyn Sink, prefix: &str, topic: &str, payload: &[u8])`：
  - `log` 进 `logs`；`node_definition` / `graph_update` 更新节点和边；
  - 其他类型：登记实体，`enabled` 时交给 `sink`；
  - 解析失败的 `malformed += 1`；
  - 新实体默认 `enabled = true`。
- `pub struct SessionOptions { pub mode: String /* "peer"|"client" */, pub connect_endpoints: Vec<String>, pub listen_endpoints: Vec<String>, pub prefix: String /* 默认 "dviz" */ }`
- `pub struct DvizSession`：
  - `pub fn global() -> &'static DvizSession`：`OnceLock`，整个进程只有一个；
  - `pub fn new() -> DvizSession`：测试用，可以建多个互不相干的会话；
  - `pub fn connect(&self, options: SessionOptions) -> Result<(), String>`：已连接时先断开再连；
  - `pub fn disconnect(&self)`；
  - `pub fn set_sink(&self, sink: Box<dyn Sink>)`；
  - `pub fn set_entity_enabled(&self, entity: &str, enabled: bool)`；
  - `pub fn snapshot(&self) -> SessionSnapshot`：`SessionSnapshot` 含 `connected: bool`、`last_error: Option<String>`、`generation: u64`（每次成功连接加 1）、`receivers_alive: u32`（活着的接收 / 工作线程组数，始终 ≤ 1）、`logs`、`nodes`、`edges`、`entities`、`dropped`、`malformed`、`received: u64`。
- **线程模型：** 照 `dorobot-studio-core/src/api/telemetry/receiver.rs`：
  - 后台线程里 `zenoh::open(config).wait()`；
  - `declare_subscriber(format!("{prefix}/**"))` 用回调方式：回调把 `(topic, payload)` 推进一个 `Mutex<VecDeque>`，容量 `RING_CAP = 64`，满了弹出最旧的并 `dropped += 1`。另一个工作线程取出来调 `route`。回调里不做解析，不能阻塞 Zenoh。`RingChannel` 丢弃时不报数，所以不用它；
  - 用 `AtomicBool` 停止；
  - `connect/timeout_ms` 为 1000。
  - `connect` 本身不能等 Zenoh 建好：失败要在后台线程里写进 `last_error`，`connect` 立即返回。

- [ ] **Step 1：写失败的测试。**
  - **`route.rs`：**
    - `log_goes_to_ring_and_is_capped`：路由 501 条 `log` 后，`logs.len() == 500`，最旧的那条被挤掉。
    - `points_reach_sink_and_register_entity`：话题 `dviz/world/lidar` 的 `points3d`，`sink.calls == [("world/lidar","points3d")]`，实体计数 1。
    - `disabled_entity_skips_sink`：`set_entity_enabled` 置为 false 后，`sink` 不再被调用，计数照样增加。
    - `malformed_and_unknown_messages_are_counted_not_fatal`：坏 JSON 使 `malformed == 1`；未知类型照样登记实体，交给 `sink`（未知类型由 `Sink` 自己忽略）。
    - `graph_update_replaces_nodes_and_edges`。
  - **`tests/loopback.rs`**（真实 Zenoh，本机 TCP，端口用 `std::net::TcpListener::bind("127.0.0.1:0")` 取空闲端口后释放）：
    - `session_receives_over_loopback`：会话以 `peer` 模式 listen `tcp/127.0.0.1:<p>`；测试里另开一个 Zenoh 会话 connect 过去，`put` 一条 `log` 和一条 `points3d`；3 s 内 `snapshot().logs` 含这条日志、`received >= 2`。
    - `connect_to_unreachable_endpoint_reports_error_without_blocking`：以 `client` 模式连 `tcp/127.0.0.1:<空闲端口>`；`connect` 在 100 ms 内返回；3 s 内 `last_error.is_some()`，`connected == false`。
    - `reconnect_replaces_the_single_receiver`：连、断、再连，`generation` 从 1 变成 2；断开后 1 s 内 `receivers_alive == 0`，再连后为 1，任何时刻都不超过 1。
    - `burst_beyond_capacity_drops_oldest_and_counts`：`sink` 每次调用睡 20 ms，一口气 `put` 500 条 `points3d`；结束后 `dropped > 0`，且 `received + dropped` 接近 500（允许 ±`RING_CAP`）。
- [ ] **Step 2：确认失败。** `cargo test -p dorobot-plugin-dviz`：编译失败或断言失败。
- [ ] **Step 3：实现** `route.rs`，然后 `session.rs`。快照只拷贝界面要的东西，日志全量拷贝也只有 500 条。
- [ ] **Step 4：确认通过。** `cargo test --locked -p dorobot-plugin-dviz` 全部通过；loopback 测试连跑 3 次都过。
- [ ] **Step 5：提交**（`feat(plugins/dviz): process-wide Zenoh session with bounded routing and snapshots`）。

### Task 4：`RerunSink`（写 Rerun）

**Files:**
- Create：`plugins/dviz/src/rerun_sink.rs`，只在 `#[cfg(feature = "rerun")]` 下编译
- 来源：dviz `dviz-shell/src/app.rs:1732-1900` 的 `log_vis_data_to_rerun`
- Test：同文件 `#[cfg(test)]`

**Interfaces：**
- `pub struct RerunSink`：
  - `pub fn spawn(app_id: &str) -> Result<RerunSink, String>`：`RecordingStreamBuilder::new(app_id).spawn()`；`app_id` 用 `"dorobot-dviz"`；
  - `pub fn memory(app_id: &str) -> (RerunSink, rerun::MemorySinkStorage)`：测试用；
  - 实现 `Sink`。
- **类型映射**照原函数：`points3d`（`xyz_f32` 二进制，或 JSON `positions`）、`boxes3d`、`arrows3d`、`linestrips3d`、`transform3d`、`scalar`、`text`；其他类型返回 `Ok(())`，直接忽略。
- 二进制点云用 Task 1 的 `parse_points_xyz_f32`，不再自己解析。

- [ ] **Step 1：写失败的测试**（都用 `memory`）：
  - `each_supported_type_logs_without_error`：上面 7 种类型各一条，都返回 `Ok`，`storage` 非空。
  - `short_point_binary_is_an_error_not_a_panic`：`count` 10、5 字节，返回 `Err`。
  - `unknown_type_is_ignored`：`"foo"` 返回 `Ok`。
  - `spawn_without_rerun_binary_returns_clear_error`：清空 `PATH` 后 `spawn`，返回 `Err`，消息里含 "rerun"。要用一个子进程或串行锁隔离环境变量改动，避免影响并行测试。
- [ ] **Step 2：确认失败。** `cargo test -p dorobot-plugin-dviz --features rerun`。
- [ ] **Step 3：实现。** 在 `lib.rs` 里：开了 `rerun` 特性时，"打开 3D 窗口"用 `RerunSink::spawn` 并 `set_sink`；没开特性时这个按钮显示"此构建未包含 Rerun"。
- [ ] **Step 4：确认通过。**
  - `cargo build -p dorobot-plugin-dviz --features rerun` 更新 `Cargo.lock`；
  - `cargo test --locked -p dorobot-plugin-dviz --features rerun` 全部通过；
  - `cargo test --locked -p dorobot-plugin-dviz` 仍然通过（不开特性）。
- [ ] **Step 5：提交**（`feat(plugins/dviz): Rerun sink ported from dviz-shell (optional feature)`）。

### Task 5：`dviz_view` 面板（连接、3D 窗口、显示列表、日志、节点列表）

**Files:**
- Modify：`plugins/dviz/src/lib.rs`，面板单独放 `plugins/dviz/src/view_panel.rs`
- Test：`dorobot-flex/tests/plugins.rs`，以及 `plugins/dviz/tests/panel.rs`

**Interfaces：**
- `DvizViewPanel::create(vm: &mut ScriptVm, session: &'static DvizSession) -> DvizViewPanel`；插件工厂传 `DvizSession::global()`。测试用 `Box::leak(Box::new(DvizSession::new()))` 拿一个独立的 `&'static`。
- **界面**（`script_eval!`，照 `plugins/policy` 的 Card 布局和 `frame::apply_light` 主题）：

  | 控件 id | 内容 |
  |---|---|
  | `mode` | `TextInput`，默认 `peer` |
  | `endpoint` | `TextInput`，例如 `tcp/192.168.1.186:7447`，空则组播发现 |
  | `prefix` | `TextInput`，默认 `dviz` |
  | `connect`、`disconnect` | 按钮 |
  | `status` | 已连接 / 错误、收到数、丢弃数、坏消息数 |
  | `open_viewer` | 按钮"打开 3D 窗口" |
  | `entities` | 显示列表：每个实体一行，带开关（具体控件由实现者选，可用 `PortalList` 或按快照重建的行）和计数 |
  | `log_level` | 全部 / WARN 及以上 |
  | `logs` | 最近 N 条，照 policy 的 `Label{max_lines: ...}` |
  | `nodes` | 节点名、状态、最后活动时间（Task 6 再加图） |
- `sync` 每次读 `session.snapshot()`；`handle_actions` 处理按钮和开关。
- `on_disable` 只停面板自己的刷新，**不调用** `disconnect`。

- [ ] **Step 1：写失败的测试**（`plugins/dviz/tests/panel.rs`，用 `Cx::new` + `register_ui` 的方式，照 `dorobot-flex/tests/plugins.rs` 的 `setup`）：
  - `connect_button_starts_the_session_with_the_entered_endpoint`：在 `endpoint` 填 `tcp/127.0.0.1:<空闲端口>`、`mode` 填 `client`，点 `connect`；`handle_actions` 返回 true，200 ms 内 `snapshot().last_error` 或 `connected` 有值（说明确实发起了连接）。
  - `disable_keeps_the_session_connected`：本机 loopback 连上后调 `on_disable`；`snapshot().connected` 仍为 true。
  - `open_viewer_failure_is_shown_and_session_keeps_running`：没开 `rerun` 特性（或清空 `PATH`）时点 `open_viewer`，`status` 的文字含错误；会话照常收日志。
  - `two_panels_share_one_session`：同一个 `&'static` 会话建两个面板，一个点 `connect`，另一个 `sync` 后 `status` 显示已连接。
  - `entity_toggle_disables_routing`：会话里已有实体 `world/lidar`；点它的开关，之后 `snapshot().entities["world/lidar"].enabled == false`。
- [ ] **Step 2：确认失败。** `cargo test -p dorobot-plugin-dviz --test panel`。
- [ ] **Step 3：实现**面板。
- [ ] **Step 4：确认通过。** `cargo test --locked -p dorobot-plugin-dviz --test panel` 和 `cargo test --locked -p dorobot-flex --no-default-features --test plugins` 都过，包括无脚本错误那条。
- [ ] **Step 5：提交**（`feat(plugins/dviz): visualization panel (connection, 3D window, displays, logs, nodes)`）。

### Task 6：数据流图（自绘）

**Files:**
- Create：`plugins/dviz/src/graph.rs`，含布局纯函数和自绘控件
- 参考：dviz `dviz-widgets/src/dataflow_graph.rs`（旧写法，只借布局和配色）、Studio `dorobot-ux` 里的自绘控件（如 `Spark`）
- Modify：`view_panel.rs`，把 `nodes` 区换成"列表 / 图"两种视图，点节点显示详情（输入、输出、状态）
- Test：`graph.rs` 的 `#[cfg(test)]`，以及 `tests/panel.rs`

**Interfaces：**
- `pub fn layout(nodes: &[GraphNode], edges: &[GraphEdge], size: (f64, f64)) -> GraphLayout`：
  - 按数据流分层：没有输入的节点在第一层，每层依次放下游节点；
  - `GraphLayout { boxes: Vec<(String /*node id*/, Rect)>, links: Vec<(Point, Point)> }`。
- `pub fn hit_test(layout: &GraphLayout, p: Point) -> Option<String>`

- [ ] **Step 1：写失败的测试。**
  - `layout_is_deterministic_and_non_overlapping`：5 个节点、4 条边的链；两次布局结果相同；任意两个框不相交；所有框都在 `size` 内。
  - `sources_are_left_of_sinks`：边 `a→b`，`a` 的 x 小于 `b` 的 x。
  - `hit_test_finds_the_node_under_the_point`：框中心命中，框外为 `None`。
  - 在 `tests/panel.rs` 加 `graph_view_builds_without_script_errors_and_shows_node_detail_on_select`。
- [ ] **Step 2：确认失败。**
- [ ] **Step 3：实现**布局、绘制（节点框按状态着色，连线）和点选。
- [ ] **Step 4：确认通过。** `cargo test --locked -p dorobot-plugin-dviz` 全部通过。
- [ ] **Step 5：提交**（`feat(plugins/dviz): dataflow graph view`）。

### Task 7：合成发布器 + 文档 + 端到端检查

**Files:**
- Create：
  - `plugins/dviz/examples/fake_publisher.rs`：`SensorSimulator` 搬自 dviz `dviz-rerun-bridge/src/simulation.rs`，放在 `plugins/dviz/examples/sim/mod.rs`；
  - `docs/DVIZ.md`（中文）。
- Modify：`docs/PLUGINS.md`，分层图里加上 dviz 插件；`NOTICE`（Task 1 已建，这里补上搬入的文件清单）。
- 另一个仓库：`~/nvme/dviz` 的 `README.md` 开头加一句"已并入 DoRobot Studio（`plugins/dviz`），本仓库不再开发"，在该仓库本地提交，不推送。

**Interfaces：**
- 用法：`cargo run --locked -p dorobot-plugin-dviz --example fake_publisher -- --listen tcp/0.0.0.0:7447 [--prefix dviz] [--hz 10]`
- 以 10 Hz 发布：
  - `points3d`（`world/lidar`，`xyz_f32` 二进制）；
  - `transform3d`（`world/vehicle`）；
  - `linestrips3d`（`world/path`）；
  - `scalar`（`imu/*`）；
  - 每秒一条 `log`；
  - 每 2 s 一条 `node_definition` / `graph_update`（3 个假节点：`sim`、`planner`、`bridge`）。

- [ ] **Step 1：写失败的测试**：在 `tests/loopback.rs` 加 `fake_publisher_feeds_every_displayed_kind`。用 `std::process::Command` 跑已编好的 example（`cargo run --example` 太慢，就把发布逻辑做成 example 里的 `pub fn run_for(duration)`，测试直接调用）；3 s 内会话里有 `world/lidar`、`world/vehicle`、`world/path` 三个实体、至少一条日志和 3 个节点。
- [ ] **Step 2：确认失败。**
- [ ] **Step 3：实现**发布器；写 `docs/DVIZ.md`：
  - 前提（`rerun-cli 0.28`）；
  - 连接方式（组播；跨 Wi-Fi 时用 `client` 模式加 `endpoint`）；
  - 合成发布器用法；
  - 已知限制（3D 是独立窗口；P2 才接 DORA_NAV）。
- [ ] **Step 4：确认通过。**
  - `cargo test --locked --workspace --no-default-features` 全部通过，和 Task 1 的基线比只多不少；
  - `cargo test --locked -p dorobot-plugin-dviz --features rerun` 全部通过；
  - `cargo build --locked -p dorobot-flex`（带默认特性）成功。
- [ ] **Step 5：手动端到端（交给用户在有桌面的机器上做，记录结果）：**
  - 起合成发布器，开 Studio，进入"3D visualization"，连接，点"打开 3D 窗口"；
  - Rerun 里能看到点云、车的位姿、路径，面板里能看到日志和 3 个节点；
  - 切到别的页面再回来，连接还在。
- [ ] **Step 6：提交**（Studio：`docs(dviz): plugin guide, fake publisher`；dviz：`docs: moved into DoRobot Studio`）。
