# dviz P2 交付②：设初始位姿、发目标、急停 / 撤防 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal：** 在 Studio 的 Navigation 2D 地图上用 `2D Pose Estimate`、`2D Goal Pose` 给 Tracer 设初始位姿、发目标，用 `E-STOP`、`Disarm` 急停、撤防；每条命令都有回执。

**Architecture：**
- **协议：** 新的请求 `CmdRequest`、回复 `CmdAck`（JSON），走 Zenoh 查询，键 `dviz/<robot_id>/cmd`。
- **桥节点：** 改成由桥节点本身持有 Zenoh 会话；声明 `queryable`。急停 / 撤防和目标 / 初始位姿分两条处理线，过滤（令牌、开关、限流、方案 A 门状态）后用 UDP 转给车上 `nav_command`（`127.0.0.1:9998`），把结果回给查询方。
- **Studio 插件：** 会话线程打开 Zenoh 后把句柄共享给界面；界面不阻塞地发查询，回调写回执；2D 地图加工具状态机和预览箭头；面板加四个按钮、回执行、工具不可用的原因行。
- DORA_NAV 不改。

**Tech Stack：** 同交付③。Rust（Makepad `44d8220f`、Zenoh `=1.10.2`、Rerun 0.28、dora-node-api `=1.1.0`）；bash；Python 3（只用现有的 `tracer_ctl.py`）。

**Spec：**
- `docs/superpowers/specs/2026-11-24-dviz-p2-commands-design.md`（下称"设计"，提交 `a97c84b`）；
- 前身：`docs/superpowers/specs/2026-11-22-dviz-p2-nav-view-design.md`；
- 交付①、③的计划：`docs/superpowers/plans/2026-11-23-dviz-p2-view.md`、`2026-11-23-dviz-p2-rviz-costmap.md`，其全局约束仍然有效。

## Global Constraints

- **交付①、③的全局约束全部有效：**
  - Makepad 版本固定；构建一律 `--locked`；Zenoh `=1.10.2`，只开 `transport_tcp`；Rerun 0.28 在 `rerun` 特性后面；
  - 桥节点是独立工作区；不拖累导航；
  - Studio 构建前缀 `LIBRARY_PATH=$HOME/nvme/dora-lab/linkshim`，并 `export TMPDIR=$HOME/nvme/dora-lab/tmp`；`dorobot-flex` 只用 `--no-default-features`；
  - 不用 Rust 1.95 之后的特性；**不加新依赖**（`Cargo.lock` 只允许因本仓库自己的包变化而变）；
  - 界面文案英文。
- **分支：** Studio `feat/dviz-p2-cmd`（当前 `a97c84b`，叠在 `feat/dviz-p2` 上）。DORA_NAV 不改、不建分支。
- **提交身份：**
  - `git -c user.name=bobdingai -c user.email=dingxinmin@gmail.com commit`；
  - 提交信息最后一行 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`；
  - 只在本地。
- **车上正在跑的东西不能动：**
  - 车上 `tracer-nav` 在跑，桥节点在 `/home/nvidia/nvme/dora-lab/opt/nav-bridge`，占着 UDP 9998 和 Zenoh 7450；
  - 桥节点 release 版用 `--target-dir $HOME/nvme/dora-lab/tmp/nav-bridge-target`；
  - 实现者不运行 `dora`、`autostart.sh` / `autostop.sh`、`systemctl`、`install.sh` 到 `opt/`、CAN 工具，**不往 9998 发任何东西**；测试里的 UDP 一律用 `127.0.0.1` 的临时端口；
  - 不用 `pgrep -f` / `pkill -f` 匹配自己命令行里的字符串；
  - 运行 dora（任务 8）、部署和实车（任务 9）只由主控在用户同意后做。
- **线上格式：**
  - 查询键 `<prefix>/<robot_id>/cmd`（桥节点固定 `dviz/<robot_id>/cmd`）；
  - 请求 JSON ≤ 1024 字节；`id` 为 1–64 个 `[A-Za-z0-9_-]`；
  - 结果代码（`snake_case`）：`ok`、`not_allowed`、`disabled`、`bad_token`、`not_armed`、`estop_active`、`gate_unknown`、`rate_limited`、`bad_request`、`nav_rejected`、`no_reply`；
  - Studio 本地结果：`not_connected`、`timeout`（不上线路）。
- **桥节点 `cmd` 配置默认值：** `goal_enabled` `false`，`token_file` `"~/.config/dviz/cmd_token"`，`forward` `"127.0.0.1:9998"`，`reply_timeout_ms` `1000`，`max_goal_per_s` `2`（令牌桶，突发 2）。
- **桥节点规则：**
  - `estop` / `disarm` 不看令牌、开关、限流、门状态，worker 死了也照样转发；从收到到发出 UDP ≤ 50 ms，不排在目标后面；
  - `arm` / `release` / 未知命令 → `not_allowed`；
  - `goal` / `initialpose` 依次检查：开关关或车上令牌不可用 → `disabled`；令牌不对 → `bad_token`；超限 → `rate_limited`；
  - 只对 `goal`：门状态从未收到、超过 2 s 或解析失败 → `gate_unknown`；`estop` 为真 → `estop_active`；`armed` 为假 → `not_armed`；
  - 车上令牌：去掉首尾空白后非空，文件权限不宽于 600（`mode & 0o077 == 0`），常数时间比较，启动时读一次；
  - `cmd_goal_enabled = goal_enabled && 车上令牌可用`；
  - 日志、回执里不出现令牌。
- **Studio 规则：**
  - 令牌 `$HOME/.config/dviz/cmd_token`，每次连接时读，去掉首尾空白，只在内存里；
  - 查询超时 3 s；急停 / 撤防用 `Priority::RealTime` 和 `CongestionControl::Block`；
  - 机器人编号只从 `<prefix>/<robot_id>/nav/status` 话题里记；断开或重连时清空；
  - `E-STOP` / `Disarm` 发给见过的每个编号；工具只在正好一个编号时可用；
  - 请求 `id` 格式 `s<序号>-<4 位 base36>`（序号进程内从 1 递增；后缀取进程启动时的 Unix 秒对 36⁴ 取模），例如 `s3-k9xq`。
- **2D 工具：**
  - 拖动阈值 10 像素：≥ 10 时 `yaw = atan2(-(y1 − y0), x1 − x0)`（屏幕 y 向下）；
  - < 10 时：Goal 发不带 `yaw` 的目标；Pose Estimate 不发，提示 `drag to set the heading`；
  - Goal 预览用目标图层颜色；Pose Estimate 预览用 `[170, 90, 255, 255]`；预览线 3 px，箭头最短 0.3 m；
  - 发送后工具退出；Esc 或再点按钮取消；工具激活时光标为十字。
- **界面文案（逐字）：**
  - 按钮：`2D Pose Estimate`、`2D Goal Pose`、`E-STOP`、`Disarm`；
  - 不可用原因：`not connected`、`no robot seen yet`、`more than one robot on this connection`、`no token (~/.config/dviz/cmd_token)`、`nav status stale`、`goal / pose commands not enabled on the robot`、`gate state unknown`、`e-stop latched (release it on the robot)`、`robot not armed (arm it on the robot)`；
  - 原因行：两个工具都不可用时 `Pose / goal tools off: <原因>`，只有 Goal 不可用时 `2D Goal Pose off: <原因>`，都可用时为空。
- **仿真：** 机器人编号 `tracer_sim`，桥节点监听 `tcp/0.0.0.0:7451`，nav_command `NAV_COMMAND_PORT=19998`。

## Review Focus

1. **worker 线程挂了以后按 E-STOP：** 急停仍被转发，并回 `ok`。测试见任务 4 `estop_forwarded_after_worker_died`。
2. **刚撤防（同一秒内）就来了目标：** 门状态的周期消息还没到，也必须拒收。测试见任务 3 `goal_rejected_right_after_disarm`。
3. **令牌文件带换行、CRLF 或首尾空格**（`echo` 写的、Windows 拷来的）：两边都去掉空白后比较，能匹配。测试见任务 2 `token_trims_whitespace_and_crlf` 和任务 5 `studio_token_trims_whitespace`。
4. **Studio 从仿真切到实车（或反过来）：** 重连后旧编号清空，急停只发给新连上的车。测试见任务 5 `robots_cleared_on_reconnect`。
5. **拖到一半按 Esc、拖到地图外松手、拖动时车被撤防：** Esc 作废不发；地图外松手按松手点发送（Makepad 按住时一直捕获指针）；被撤防时工具退出、不发。测试见任务 6 `escape_during_drag_cancels`、`release_outside_map_still_sends`、`unavailable_during_drag_cancels`。

---

## 文件结构（Studio `feat/dviz-p2-cmd`）

| 文件 | 职责 | 任务 |
|---|---|---|
| `crates/dviz-protocol/src/cmd.rs`（新）、`src/lib.rs` | 请求、回复、结果代码、键 | 1 |
| `runtime/nav-bridge/src/command.rs`（新）、`src/config.rs` | 纯逻辑：判定、令牌、令牌桶、门状态解读、`cmd` 配置 | 2 |
| `runtime/nav-bridge/src/forward.rs`（新） | UDP 转发、门状态槽位、两条处理线 | 3 |
| `runtime/nav-bridge/src/{publish,run,worker,status}.rs`、`src/main.rs`、`config/tracer.json`、`examples/dviz_cmd.rs`（新）、`README.md`、`tests/zenoh_loop.rs` | 会话归属、`queryable`、状态字段、命令行工具 | 4 |
| `plugins/dviz/src/{route,session}.rs` | 机器人编号、令牌、发命令、回执 | 5 |
| `plugins/dviz/src/nav_tools.rs`（新）、`src/lib.rs` | 工具状态机、拖动换算、可用性、回执文案 | 6 |
| `plugins/dviz/src/{map2d,nav_panel}.rs`、`docs/DVIZ.md` | 2D 工具和面板 | 7 |
| `runtime/nav-bridge/tests/sim/`（新） | 仿真数据流和命令核对脚本 | 8 |
| 进度文档 `docs/superpowers/plans/2026-11-24-dviz-p2-commands-progress.md`（新） | 部署、实车记录 | 9 |

---

### Task 1: 协议的命令类型

**Files:**
- Create: `crates/dviz-protocol/src/cmd.rs`
- Modify: `crates/dviz-protocol/src/lib.rs`（`pub mod cmd; pub use cmd::*;`；`NavStatus.cmd_goal_enabled` 的文档注释改成"车上已开放 goal / initialpose：配置开关打开且车上令牌可用"）
- Test: `cmd.rs` 里的 `mod tests`

**Interfaces（Produces）：**

```rust
pub const CMD_ENTITY: &str = "cmd";
pub const CMD_MAX_BYTES: usize = 1024;
pub const CMD_ID_MAX: usize = 64;
pub fn cmd_key(prefix: &str, robot_id: &str) -> String;          // "{prefix}/{robot_id}/cmd"

#[derive(Clone, PartialEq, Serialize, Deserialize)]               // Debug 手写，令牌显示为 "<redacted>"
pub struct CmdRequest {
    pub cmd: String, pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub yaw: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdKind { Goal, InitialPose, Estop, Disarm, Other }     // "goal" "initialpose" "estop" "disarm"，其余 Other
impl CmdRequest { pub fn kind(&self) -> CmdKind; }
/// 大小、JSON、id 规则、按种类检查字段（goal：x、y 有限，yaw 有则有限；initialpose：x、y、yaw 都有且有限）。Other 不检查坐标。
pub fn parse_cmd_request(bytes: &[u8]) -> Result<CmdRequest, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmdResult { Ok, NotAllowed, Disabled, BadToken, NotArmed, EstopActive, GateUnknown, RateLimited, BadRequest, NavRejected, NoReply }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CmdAck { pub id: String, pub cmd: String, pub result: CmdResult, pub reason: String }
pub fn parse_cmd_ack(bytes: &[u8]) -> Result<CmdAck, String>;     // ≤ 1024 字节
```

- [ ] **Step 1：写失败的测试：**
  - `cmd_key_layout`：`cmd_key("dviz", "tracer") == "dviz/tracer/cmd"`。
  - `request_round_trip`：四种命令各一条，序列化再 `parse_cmd_request` 相等；`estop` 的 JSON 里没有 `x`、`token` 键。
  - `bad_requests_are_rejected`：每条都是 `Err`：1025 字节、不是 JSON、`id` 为空、`id` 65 字符、`id` 含 `/`、goal 缺 `y`、goal 的 `x` 为 `1e400`（JSON 解析成无穷）、initialpose 缺 `yaw`。
  - `unknown_and_forbidden_commands_parse`：`{"cmd":"arm","id":"a1"}`、`{"cmd":"release","id":"a2"}`、`{"cmd":"fly","id":"a3"}` 都能解出来，`kind() == CmdKind::Other`。
  - `ack_round_trip_and_codes`：`CmdResult::GateUnknown` 序列化为 `"gate_unknown"`；11 个代码都能往返；未知代码 `"weird"` 解析为 `Err`。
  - `debug_redacts_token`：`format!("{:?}", req)` 不含令牌原文，含 `<redacted>`。
- [ ] **Step 2：确认失败：** `LIBRARY_PATH=$HOME/nvme/dora-lab/linkshim cargo test --locked -p dviz-protocol cmd`
- [ ] **Step 3：实现** `cmd.rs`（坐标检查用 `f64::is_finite`）。
- [ ] **Step 4：确认通过：** `cargo test --locked -p dviz-protocol`；`cargo clippy --locked -p dviz-protocol --all-targets` 无警告。
- [ ] **Step 5：提交** `feat(dviz-protocol): command request and reply for Zenoh queries`

### Task 2: 桥节点的命令判定（纯逻辑）

**Files:**
- Create: `runtime/nav-bridge/src/command.rs`
- Modify: `runtime/nav-bridge/src/config.rs`（`BridgeConfig.cmd: CmdConfig`，`#[serde(default)]`）、`src/lib.rs`（`pub mod command;`）
- Test: `command.rs`、`config.rs` 里的 `mod tests`

**Interfaces：**
- Consumes：任务 1 的 `CmdRequest`、`CmdKind`、`CmdResult`。
- Produces：

```rust
// config.rs
#[derive(Debug, Clone, PartialEq, Deserialize)] #[serde(default)]
pub struct CmdConfig { pub goal_enabled: bool, pub token_file: String, pub forward: String, pub reply_timeout_ms: u64, pub max_goal_per_s: f32 }
// command.rs
pub const GATE_FRESH: Duration = Duration::from_secs(2);
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum GateView { Unknown, Armed, Disarmed, Estop }
pub fn gate_view(latest: Option<(&[u8], Instant)>, now: Instant) -> GateView;  // cmd_gate/state JSON {"armed","estop",...}
pub struct TokenBucket { /* … */ }
impl TokenBucket { pub fn new(per_s: f32, burst: u32) -> Self; pub fn take(&mut self, now: Instant) -> bool; }
pub struct Token(/* 私有 */);                                     // 不实现 Debug / Display
impl Token { pub fn matches(&self, given: &str) -> bool; }        // 两边 trim 后常数时间比较
pub fn load_token(path: &Path) -> Result<Token, String>;          // 不存在 / 空 / 权限过宽 → Err（原因里不含内容）
pub fn expand_home(path: &str, home: Option<&Path>) -> PathBuf;   // 只展开开头的 "~/"
pub struct CmdPolicy { pub goal_enabled: bool, pub token: Option<Token>, pub bucket: TokenBucket }
impl CmdPolicy { pub fn goal_channel_open(&self) -> bool; }       // goal_enabled && token.is_some()
#[derive(Debug, PartialEq)] pub enum Decision { Forward(Vec<u8>), Reply(CmdResult, String) }
pub fn decide(req: &CmdRequest, policy: &mut CmdPolicy, gate: GateView, now: Instant) -> Decision;
pub fn nav_command_payload(req: &CmdRequest) -> Vec<u8>;          // {"cmd","x","y","yaw"?,"id"?}；goal 带 id；从不含 token
```

- [ ] **Step 1：写失败的测试：**
  - `cmd_config_defaults`：`CmdConfig::default()` 等于全局约束里的默认值；`config/tracer.json` 里没有 `cmd` 段时也一样。
  - `estop_and_disarm_always_forward`（表格）：开关关、`token: None`、令牌桶已空、`GateView::Unknown` / `Estop` / `Disarmed`、请求不带令牌，`estop` 和 `disarm` 都得到 `Decision::Forward`，且不消耗令牌桶。
  - `arm_release_unknown_not_allowed`：`Reply(NotAllowed, _)`，即使令牌对、开关开。
  - `goal_checks_in_order`（表格，每行一种情况）：开关关 → `Disabled`；`token: None` → `Disabled`；令牌错 → `BadToken`；第 3 条（1 s 内）→ `RateLimited`；门 `Unknown` → `GateUnknown`；`Estop` → `EstopActive`；`Disarmed` → `NotArmed`；`Armed` → `Forward`。
  - `initialpose_ignores_gate`：门 `Disarmed`、`Estop`、`Unknown` 时都 `Forward`。
  - `gate_view_rules`：`{"armed":true,"estop":false}` 1 s 前 → `Armed`；2.5 s 前 → `Unknown`；`{"armed":true,"estop":true}` → `Estop`；`{"armed":false}` → `Disarmed`；坏 JSON → `Unknown`；`None` → `Unknown`。
  - `token_bucket_two_per_second`：同一时刻 `take` 3 次得 `[true, true, false]`；0.5 s 后再得一次 `true`。
  - `token_trims_whitespace_and_crlf`：文件内容 `"abc123\r\n"`，`matches("  abc123 ")` 为真，`matches("abc12")` 为假。
  - `load_token_rejects`：不存在、只有空白、权限 0644 → `Err`；0600 → `Ok`。
  - `payload_never_has_token`：`nav_command_payload` 的结果里没有 `token` 键；goal 带 `"id"`；没有 `yaw` 的 goal 不带 `yaw` 键。
- [ ] **Step 2：确认失败：** `cd runtime/nav-bridge && cargo test --locked command`
- [ ] **Step 3：实现。** `reason` 用简短英文，例如 `"goal / initialpose not enabled on the robot"`、`"robot not armed"`；任何 `reason` 都不拼令牌。
- [ ] **Step 4：确认通过：** `cargo test --locked`；`cargo clippy --locked --all-targets` 无警告。
- [ ] **Step 5：提交** `feat(nav-bridge): command decision: token, rate limit, gate rule A`

### Task 3: 转发、门状态槽位和两条处理线

**Files:**
- Create: `runtime/nav-bridge/src/forward.rs`
- Modify: `runtime/nav-bridge/src/lib.rs`
- Test: `forward.rs` 里的 `mod tests`（假 nav_command：测试线程里绑定 `127.0.0.1:0` 的 UDP 套接字）

**Interfaces：**
- Consumes：任务 2 的 `decide`、`CmdPolicy`、`GateView`、`gate_view`、`Decision`；任务 1 的 `CmdAck`、`CmdResult`、`CmdRequest`。
- Produces：

```rust
pub struct GateSlot { /* Mutex<Option<(Vec<u8>, Instant)>> */ }
impl GateSlot { pub fn new() -> Self; pub fn store(&self, bytes: &[u8], now: Instant); pub fn view(&self, now: Instant) -> GateView; }
#[derive(Debug, PartialEq)] pub enum Forwarded { Ok, Rejected, NoReply }
/// 每次用新的 127.0.0.1:0 套接字发一条，等 {"ok":bool} 至多 timeout。
pub fn forward_udp(target: SocketAddr, payload: &[u8], timeout: Duration) -> Forwarded;
pub trait Replier: Send { fn reply(self: Box<Self>, ack: &CmdAck); }
pub struct Job { pub request: Result<CmdRequest, String>, pub replier: Box<dyn Replier> }
pub struct CommandLanes { /* 两个 sync_channel(16) 和两个线程 */ }
impl CommandLanes {
    /// estop/disarm 走 stop 线，其余（含坏请求）走 main 线；每条：decide → forward_udp → reply → 写一行日志。
    pub fn spawn(policy: CmdPolicy, gate: Arc<GateSlot>, target: SocketAddr, timeout: Duration, log: Box<dyn Write + Send>) -> Self;
    pub fn submit(&self, job: Job) -> bool;                      // 线满时 false（查询方会超时）
    pub fn stop(self);                                           // 关通道、等两个线程结束
}
```

日志行格式：`nav-bridge: cmd <cmd> <id> [x=… y=… yaw=…] -> <result>`（坐标保留 2 位小数，没有就省略），不含令牌。

- [ ] **Step 1：写失败的测试：**
  - `forward_ok_rejected_and_silence`：假 nav_command 依次回 `{"ok":true}`、`{"ok":false}`、不回；结果 `Ok`、`Rejected`、`NoReply`（超时 200 ms，测量耗时 < 400 ms）。
  - `goal_rejected_right_after_disarm`：槽位先存 `{"armed":true,"estop":false}`，再存 `{"armed":false,"estop":false}`（同一时刻），随后提交令牌正确的 goal：回执 `NotArmed`，假 nav_command 什么都没收到。
  - `estop_not_queued_behind_goal`：假 nav_command 对 goal 不回复、对 estop 立即回复；先提交 goal（超时 1000 ms），10 ms 后提交 estop：estop 的 UDP 在提交后 50 ms 内到达假 nav_command，回执 `Ok`。
  - `bad_request_gets_reply`：`Job { request: Err(..) }` 得到 `BadRequest`，`id` 为空字符串。
  - `log_has_no_token`：一轮 goal（令牌 `"s3cr3t-token"`）、estop、bad_token 之后，日志缓冲里不含 `s3cr3t`，每条命令一行。
  - `full_lane_refuses`：stop 线被一个永不返回的 `Replier` 占住时，第 17 条之后 `submit` 返回 `false`，不阻塞。
- [ ] **Step 2：确认失败：** `cargo test --locked forward`
- [ ] **Step 3：实现。** 两条线各一个线程、一个容量 16 的 `sync_channel`，`submit` 用 `try_send`；门状态只在 main 线判定 goal 时读。
- [ ] **Step 4：确认通过：** `cargo test --locked`（`forward` 测试连跑 3 遍不失败）；clippy 无警告。
- [ ] **Step 5：提交** `feat(nav-bridge): UDP forward to nav_command on two lanes; latest gate state slot`

### Task 4: 桥节点接线（会话归属、queryable、状态、命令行工具、文档）

**Files:**
- Modify: `runtime/nav-bridge/src/publish.rs`：拆出 `pub fn open_session(cfg: &BridgeConfig) -> Result<zenoh::Session, String>` 和 `pub fn zenoh_publisher_on(session: &zenoh::Session, cfg: &BridgeConfig) -> Result<Box<dyn Publish>, String>`（持有会话克隆）；`zenoh_publisher(cfg)` 保留为两者的组合，供现有测试用。
- Modify: `runtime/nav-bridge/src/run.rs`：
  - `Bridge` 新增字段：会话、`queryable`、`CommandLanes`、`Arc<GateSlot>`、门状态输入名列表（配置 `inputs` 里格式为 `gate_state` 的输入 id）；
  - `Bridge::start` 依次：打开会话 → 读令牌（只在 `goal_enabled` 时读；结果打印一行：`nav-bridge: goal/initialpose commands enabled` 或 `… closed: <原因>`）→ 起 worker → 起 `CommandLanes` → 声明 `queryable`。`queryable` 的回调只做 `parse_cmd_request` 和 `submit`，回复由线程用 `query.reply(query.key_expr().clone(), bytes)` 完成；
  - `on_event`：输入 id 在门状态列表里时，**先** `gate.store(bytes, now)`，再走原来的收件箱逻辑（worker 死了也存）；
  - `wait(deadline)`：停 worker → `CommandLanes::stop` → 撤销 `queryable` → 关会话，整体受 `deadline` 约束（关会话放在辅助线程里，超时就不等）。
- Modify: `src/main.rs`：只改调用和启动信息里的一句话，保留 `autostart.sh` 依赖的 `nav-bridge: started by Dora` 原文。
- Modify: `src/worker.rs`、`src/status.rs`：`StatusBuilder::new(robot_model: Option<String>, cmd_goal_enabled: bool)`，每条状态都带上。
- Modify: `config/tracer.json`：加 `"cmd": { "goal_enabled": false, "token_file": "~/.config/dviz/cmd_token", "forward": "127.0.0.1:9998", "reply_timeout_ms": 1000, "max_goal_per_s": 2 }`。
- Create: `examples/dviz_cmd.rs`：`dviz_cmd --connect tcp/<host>:<port> --robot <id> --cmd <goal|initialpose|estop|disarm|arm|release> [--x X --y Y --yaw RAD] [--id ID] [--token-file PATH] [--wait-goal-state STATE --wait-s N]`。打印回复 JSON；带 `--wait-goal-state` 时，再订阅 `dviz/<robot>/nav/status`，直到 `goal_id == id` 且 `goal_state == STATE` 才以 0 退出，超时以 1 退出。
- Modify: `README.md`：新的一节"命令通道"，写明：`cmd` 配置、令牌文件和权限、怎么开、规则表、`dviz_cmd` 用法、已知局限（明文令牌；撤防和目标之间约 0.1–0.2 s 的空隙）。
- Test: `tests/zenoh_loop.rs`、`run.rs` 和 `status.rs` 里的 `mod tests`

**Interfaces：**
- Consumes：任务 1–3 的全部；`dviz_protocol::cmd_key`。
- Produces（给任务 8、9）：`dviz_cmd` 例子程序；`NavStatus.cmd_goal_enabled` 的真实值。

- [ ] **Step 1：写失败的测试**（`zenoh_loop.rs` 照现有写法：本机回环、临时端口、串行锁；假 nav_command 用 `127.0.0.1:0`，配置里的 `forward` 指向它）：
  - `query_round_trip_goal_ok`：令牌文件 0600、`goal_enabled` 为真，门状态输入先送一条 `{"armed":true,"estop":false}`；客户端会话对 `dviz/<robot>/cmd` 发带令牌的 goal → 回复 `ok`；假 nav_command 收到的 JSON 里 `id` 等于请求 id、没有 `token`。
  - `estop_works_with_channel_closed`：`goal_enabled` 为假、没有令牌文件 → `estop` 回 `ok`，`goal` 回 `disabled`。
  - `estop_forwarded_after_worker_died`：用一个第一次 `put` 就 panic 的 `Publish` 起桥节点，让 worker 死掉，然后发 `estop` → 回 `ok`，假 nav_command 收到。（需要一个能注入 `Publish` 的启动函数；在 `run.rs` 加 `pub fn start_with(cfg, sources, session: zenoh::Session, publisher: Box<dyn Publish>) -> Result<Bridge, String>`，`start` 调它。）
  - `status_reports_cmd_goal_enabled`：开关开且令牌可用时，订阅到的 `nav/status` 里 `cmd_goal_enabled == true`；开关开但令牌文件权限 0644 时为 `false`。
  - `gate_input_updates_slot_even_when_worker_dead`（`run.rs` 单元测试，`with_publisher` 用会 panic 的发布器）：worker 死后 `on_event` 送门状态，`GateSlot::view` 反映新值。
  - `stop_closes_within_deadline`：`wait(now + 1 s)` 返回 `true`，之后对 `cmd` 的查询没有回复。
- [ ] **Step 2：确认失败：** `cargo test --locked --test zenoh_loop`；`cargo test --locked run::`
- [ ] **Step 3：实现。**
- [ ] **Step 4：确认通过：**
  - `cargo test --locked`（`zenoh_loop` 连跑 3 遍）；`cargo clippy --locked --all-targets` 无警告；
  - `cargo build --locked --release --target-dir $HOME/nvme/dora-lab/tmp/nav-bridge-target --bins --examples` 成功；
  - 现有的回放模板检查：`RENDER_ONLY=1 bash tests/replay/run_replay.sh` 通过（不起 dora）。
- [ ] **Step 5：提交** `feat(nav-bridge): command channel over a Zenoh queryable; bridge owns the session; dviz_cmd example`

### Task 5: Studio 会话：机器人编号、令牌、发命令

**Files:**
- Modify: `plugins/dviz/src/route.rs`：`SessionState.robots: BTreeSet<String>`。收到 `<robot>/nav/status`（`robot` 是一段、非空、合法 id 字符）时加入；`start_connection` 和 `SessionState::default` 时清空。
- Modify: `plugins/dviz/src/session.rs`：
  - `Shared` 加 `cmd: Mutex<CmdLink>`：`session: Option<(zenoh::Session, String /*prefix*/)>`、`token: Option<String>`、`last: Option<CommandRecord>`、`seq: u64`；
  - `receive` 打开会话后放入句柄，退出前（撤销订阅前）清掉；`connect` 时调用 `load_cmd_token`；
  - 快照加 `command`、`has_token`、`robots`。
- Test: `route.rs`、`session.rs` 里的 `mod tests`

**Interfaces：**
- Consumes：任务 1 的 `CmdRequest`、`CmdAck`、`parse_cmd_ack`、`cmd_key`。
- Produces（给任务 6、7）：

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavCommand { Goal { x: f64, y: f64, yaw: Option<f64> }, InitialPose { x: f64, y: f64, yaw: f64 }, Estop, Disarm }
#[derive(Debug, Clone, PartialEq)]
pub enum CommandOutcome { Pending, Ack(CmdAck), NotConnected(&'static str), Timeout }   // NotConnected 的原因取全局约束里的文案
#[derive(Debug, Clone, PartialEq)]
pub struct CommandRecord { pub id: String, pub cmd: &'static str, pub sent: Instant, pub replies: Vec<(String /*robot*/, CommandOutcome)> }
impl DvizSession { pub fn send_command(&self, command: NavCommand) -> String; }   // 立即返回 id
pub fn load_cmd_token(path: &Path) -> Option<String>;
pub fn command_id(seq: u64, started_unix_s: u64) -> String;                        // "s{seq}-{4 位 base36}"
// SessionSnapshot 新字段：pub command: Option<CommandRecord>, pub has_token: bool, pub robots: Vec<String>
```

`send_command` 的规则：
- 没有会话或没见过编号 → 记 `NotConnected("not connected")` / `NotConnected("no robot seen yet")`；
- `Goal` / `InitialPose` 在见过不止一个编号时 → `NotConnected("more than one robot on this connection")`，不发；
- 否则对每个目标编号发 `get(cmd_key(prefix, robot)).payload(json).timeout(3 s)`，急停 / 撤防加 `Priority::RealTime` 和 `CongestionControl::Block`；
- 回调里解析回复，写进 `last`（只写 `id` 等于当前 `last.id` 的）；
- 闭包持有一个守卫，查询结束时（闭包被丢弃）该编号仍为 `Pending` 就改成 `Timeout`。

- [ ] **Step 1：写失败的测试：**
  - `robots_from_status_topics`（route）：`dviz/tracer/nav/status` → `{"tracer"}`；`dviz/tracer/robot/pose` 不加；`dviz/a/b/nav/status` 不加。
  - `robots_cleared_on_reconnect`：加过 `tracer` 后 `start_connection()` → 空。
  - `studio_token_trims_whitespace`：文件 `" t0k\r\n"` → `Some("t0k")`；空白文件、不存在 → `None`。
  - `command_ids_unique_and_short`：`command_id(3, 0) == "s3-0000"`；1..=1000 互不相同、都 ≤ 64 且只含 `[A-Za-z0-9_-]`。
  - `estop_without_connection_is_not_connected`：未连接时 `send_command(Estop)` 立即返回，快照里是 `NotConnected("not connected")`。
  - 回环 Zenoh（测试里起一个 peer 会话监听临时端口，声明 `dviz/tracer/cmd` 的 `queryable`，并按 5 Hz 发一条最小的 `dviz/tracer/nav/status`；`DvizSession::new()` 用 client 模式连它）：
    - `command_round_trip_over_zenoh`：`send_command(Goal{..})` 后 2 s 内快照里是 `Ack(ok)`；假 `queryable` 收到的请求带着令牌（测试先在临时 HOME 里写令牌文件，`load_cmd_token` 的路径由测试传入）；
    - `no_queryable_times_out`：假 peer 不声明 `queryable` → 3.5 s 内变成 `Timeout`；
    - `estop_goes_to_every_robot`：假 peer 同时发 `tracer` 和 `tracer_sim` 的状态并各声明一个 `queryable` → `send_command(Estop)` 两个都收到，记录里有两条回复；`send_command(Goal{..})` → `NotConnected("more than one robot on this connection")`，两个 `queryable` 都没收到。
  - `snapshot_has_no_token`：`format!("{:?}", snapshot)` 不含令牌原文。
- [ ] **Step 2：确认失败：** `LIBRARY_PATH=$HOME/nvme/dora-lab/linkshim cargo test --locked -p dorobot-plugin-dviz session:: route::`
- [ ] **Step 3：实现。** 令牌路径：`connect` 用 `$HOME/.config/dviz/cmd_token`；为了测试，`DvizSession` 加一个 `pub(crate) fn set_token_path(&self, path: PathBuf)`。
- [ ] **Step 4：确认通过：** 插件两种构建的测试（`-p dorobot-plugin-dviz`，以及加 `--features rerun`）；回环测试连跑 3 遍；`cargo clippy --locked -p dorobot-plugin-dviz --all-targets` 无警告。
- [ ] **Step 5：提交** `feat(dviz): send navigation commands as Zenoh queries; robots and token`

### Task 6: 工具状态机、可用性和回执文案（插件纯计算）

**Files:**
- Create: `plugins/dviz/src/nav_tools.rs`
- Modify: `plugins/dviz/src/lib.rs`（`mod nav_tools;`）
- Test: `nav_tools.rs` 里的 `mod tests`

**Interfaces：**
- Consumes：任务 5 的 `NavCommand`、`CommandRecord`、`CommandOutcome`；`map_view::MapView::screen_to_world`；`dviz_protocol::NavStatus`、`nav_status_parts::GATE`。
- Produces（给任务 7）：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ToolKind { PoseEstimate, Goal }
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolState { Idle, Armed(ToolKind), Dragging { kind: ToolKind, start_world: [f64; 2], start_screen: [f64; 2], now_screen: [f64; 2] } }
pub const TOOL_MIN_DRAG_PX: f64 = 10.0;
pub const POSE_PREVIEW_COLOR: [u8; 4] = [170, 90, 255, 255];
#[derive(Debug, PartialEq)] pub enum ToolOutcome { Nothing, Send(NavCommand), Hint(&'static str) }
impl ToolState {
    pub fn toggle(&mut self, kind: ToolKind);                       // Idle/其他工具 → Armed(kind)；同一工具 → Idle
    pub fn press(&mut self, screen: [f64; 2], world: [f64; 2]);     // Armed → Dragging
    pub fn moved(&mut self, screen: [f64; 2]);
    pub fn release(&mut self, screen: [f64; 2]) -> ToolOutcome;     // 见全局约束的 10 像素规则；Send 后回 Idle，Hint 后回 Armed
    pub fn cancel(&mut self);                                       // Esc、工具不可用 → Idle
    pub fn active(&self) -> Option<ToolKind>;
}
pub fn drag_yaw(start_screen: [f64; 2], end_screen: [f64; 2]) -> f64;
pub struct ToolInputs<'a> { pub connected: bool, pub robots: usize, pub has_token: bool, pub status: Option<&'a NavStatus>, pub status_age: Option<Duration> }
#[derive(Debug, PartialEq)] pub struct Availability { pub pose: Result<(), &'static str>, pub goal: Result<(), &'static str> }
pub fn tool_availability(inputs: &ToolInputs) -> Availability;      // 设计 §3.2 的表，按顺序取第一条
pub fn availability_text(a: &Availability) -> String;               // 全局约束里的原因行格式
pub fn command_text(record: &CommandRecord, now: Instant) -> String;
pub fn command_is_error(record: &CommandRecord) -> bool;            // 有任何一条回复不是 Pending、也不是 Ack(ok)
```

`command_text` 的格式：
- 一个编号：`Last command: <cmd> <id> → <结果>`；
- 多个编号：`Last command: <cmd> <id> → <robot>: <结果>; <robot>: <结果>`；
- 末尾加 `  (<n> s ago)`；
- `<结果>`：
  - `Pending` → `sending…`；
  - `Ack` → `<result 代码>: <reason>`；
  - `NotConnected(r)` → `not sent: <r>`；
  - `Timeout` → `no reply (robot not reachable)`。

- [ ] **Step 1：写失败的测试：**
  - `goal_drag_sends_pose_with_yaw`：`toggle(Goal)`、`press([100,100],[1.0,2.0])`、`release([100,50])` → `Send(Goal{x:1.0,y:2.0,yaw:Some(π/2)})`（屏幕向上 = 地图 +y），之后 `active() == None`。
  - `short_goal_click_has_no_yaw`：拖 5 像素 → `Send(Goal{yaw: None, ..})`。
  - `short_pose_click_hints_and_stays`：`PoseEstimate` 拖 5 像素 → `Hint("drag to set the heading")`，`active() == Some(PoseEstimate)`。
  - `escape_during_drag_cancels`：拖动中 `cancel()` → `Idle`，之后 `release` 得到 `Nothing`。
  - `release_outside_map_still_sends`：松手点在负坐标（地图外）→ 照常 `Send`，`yaw` 按松手点算。
  - `unavailable_during_drag_cancels`：拖动中调用 `cancel()`（任务 7 在车被撤防时这样调用）→ 不发送。
  - `toggle_same_tool_cancels_other_switches`：`toggle(Goal)` 两次 → `Idle`；`toggle(Goal)` 再 `toggle(PoseEstimate)` → `Armed(PoseEstimate)`。
  - `availability_table`（表格，每行断言 `pose` 和 `goal` 两个结果）：未连接；连接但 0 个编号；2 个编号；无令牌；状态 3 s 前；`cmd_goal_enabled` 为假；`stale` 含 `"gate"`；`estop`；未武装；全部满足 → 两个都 `Ok`。原因字符串逐字等于全局约束里的文案。
  - `availability_text_formats`：两个都不可用 → `"Pose / goal tools off: robot not armed …"` 这种格式；只有 Goal 不可用 → `"2D Goal Pose off: …"`；都可用 → `""`。
  - `command_text_cases`：`Pending`、`Ack(ok)`、`Ack(not_armed)`、`NotConnected`、`Timeout`、两个编号各一条，逐字比较。
  - `command_error_flag`：`Pending`、`Ack(ok)` 为假；`Ack(not_armed)`、`NotConnected`、`Timeout`、两个编号里一个 `ok` 一个 `Timeout` 为真。
- [ ] **Step 2：确认失败：** `cargo test --locked -p dorobot-plugin-dviz nav_tools`
- [ ] **Step 3：实现。**
- [ ] **Step 4：确认通过：** 插件两种构建的测试；clippy 无警告。
- [ ] **Step 5：提交** `feat(dviz): pose / goal tool state machine, availability and command text`

### Task 7: 2D 工具和面板（界面）

**Files:**
- Modify: `plugins/dviz/src/map2d.rs`：
  - `DvizMap2D` 加 `#[rust] tool: ToolState`；
  - 工具激活时：`FingerDown` 拿键盘焦点（`cx.set_key_focus(area)`），交给 `press`，不平移；`FingerMove` → `moved`，重画；`FingerUp` → `release`；`KeyDown(Escape)` → `cancel`；滚轮缩放照旧；光标 `MouseCursor::Crosshair`；
  - 拖动中画预览：起点圆点，加一条从起点指向当前指针的箭头（地图长度至少 0.3 m，3 px；Goal 用 `layer_color(scene, NAV_GOAL)`，Pose Estimate 用 `POSE_PREVIEW_COLOR`）；
  - 结果用小部件动作发出：`#[derive(Clone, Debug, Default)] pub enum DvizMapAction { #[default] None, Send(NavCommand), Hint(&'static str) }`；
  - `DvizMap2DRef` 加：`set_tool(&self, cx, Option<ToolKind>)`、`tool(&self) -> Option<ToolKind>`、`toggle_tool(&self, cx, ToolKind)`、`sent(&self, actions: &Actions) -> Option<NavCommand>`、`hint(&self, actions: &Actions) -> Option<&'static str>`。
- Modify: `plugins/dviz/src/nav_panel.rs`：
  - 工具栏加 `pose_tool := mod.widgets.ux.SegmentButton{text: "2D Pose Estimate"}`、`goal_tool := mod.widgets.ux.SegmentButton{text: "2D Goal Pose"}`、`estop := …{text: "E-STOP"}`、`disarm := …{text: "Disarm"}`；
  - `E-STOP` 用 `mod.widgets.nx.HotBtn`，`Disarm` 用 `mod.widgets.nx.MiniHot`。如果 dviz 的脚本作用域里拿不到 `nx`，在 dviz 里定义同样颜色（`#xFF453A` / `#xD70015`，按 `light` 混合）的按钮；
  - 状态行下面加 `command := DvizLabel{width: Fill}`（`command_text`，没有命令时为空）和 `tool_reason := DvizLabel{width: Fill}`（`availability_text`）；两个都加进主题列表 `THEMED`；
  - `command_is_error` 为真时 `command` 行用红色（`#xFF453A` / `#xD70015` 按 `light` 混合），照状态行 `stale` 变量的做法加一个 `error` 变量；
  - `refresh`：算 `tool_availability`；工具不可用时 `map.set_tool(cx, None)`；两个工具按钮的选中样式跟 `map.tool()` 走（照 `sync_follow_button`）；不可用的工具按钮显示为灰色，点击无效；
  - `handle_actions`：
    - 点工具按钮 → `map.toggle_tool`（不可用时忽略）；
    - `E-STOP` / `Disarm` → `session.send_command(NavCommand::Estop / Disarm)`，永远可点；
    - `map.sent(actions)` → `session.send_command(cmd)`；
    - `map.hint(actions)` → 在 `tool_reason` 行显示 2 s。
- Modify: `docs/DVIZ.md`：
  - 第 135 行那句改成现在的说法；
  - 新加"命令"一节：令牌怎么生成（`umask 077; head -c 24 /dev/urandom | base64 > ~/.config/dviz/cmd_token`，车上和操作员机器各一份）、车上怎么打开（`cmd.goal_enabled`）、四个按钮做什么、变灰原因表、已知局限（明文令牌、0.1–0.2 s 空隙、Wi-Fi 急停不是安全装置）。
- Test: `nav_panel.rs`、`map2d.rs` 里的 `mod tests`

**Interfaces：**
- Consumes：任务 5 的 `send_command`、快照新字段；任务 6 的全部。
- Produces：用户可见的界面。

- [ ] **Step 1：写失败的测试：**
  - `themed_includes_command_labels`：`THEMED` 里有 `command`、`tool_reason`（照现有主题测试的写法）。
  - `preview_plan_while_dragging`（`map2d`）：给一个 `ToolState::Dragging`，计算出的预览绘制步骤包含一个点和一组箭头笔画，颜色分别按 Goal / Pose Estimate 取。为此把预览拆成纯函数 `pub fn tool_preview(state: &ToolState, view: &MapView, scene: &NavScene) -> Vec<MapDraw>`。
  - `no_preview_when_idle`：`Idle`、`Armed` 时为空。
- [ ] **Step 2：确认失败：** `cargo test --locked -p dorobot-plugin-dviz themed_includes_command_labels preview`
- [ ] **Step 3：实现。**
- [ ] **Step 4：确认通过：**
  - 插件两种构建的测试，clippy 无警告；
  - `cargo test --locked -p dorobot-flex --no-default-features --test plugins`；
  - 有本地显示时，主控在 epyc 上按任务 9 的端到端看一遍；实现者不开窗口。
- [ ] **Step 5：提交** `feat(dviz): 2D Pose Estimate / 2D Goal Pose tools, E-STOP and Disarm in the Navigation panel`

### Task 8: 仿真数据流和命令核对（实现者写文件，主控运行 dora）

**Files（全部新建，在 `runtime/nav-bridge/tests/sim/`）：**
- `nav_sim_cmd.yml.in`：从 DORA_NAV `apps/nav_sim2d.yml` 改出来，用 `@DORA_NAV@`、`@BRIDGE@`、`@HERE@`、`@WORK@` 占位：
  - 节点 `sim2d`、`goal_resume`、`astar_planner`、`pure_pursuit`、`cmd_gate` 和原文件一样；
  - `goal_resume.goal` ← `nav_command/goal`；`cmd_gate.arm` ← `nav_command/arm`；`cmd_gate.estop` ← `nav_command/estop`；
  - `sim_mission` 用 `mission_idle.json`，只接给 `trust`、`refine_state` 的消费者；
  - 新增 `nav_command`（`@DORA_NAV@/modules/interface/nav_command/nav_command.py`，`env: NAV_COMMAND_PORT: "19998"`，`tick: dora/timer/millis/100`）；
  - 新增 `viz_bridge`（`@BRIDGE@ @WORK@/bridge_sim.json`），输入照实车的接法，位姿和激光来自 `sim2d`，`gate_state` 来自 `cmd_gate/state`。
- `mission_idle.json`：`{"goals": [], "arm_at_s": 1e9, "timeout_s": 1e9}`。
- `bridge_sim.json.in`：`robot_id` `tracer_sim`，`listen` `["tcp/0.0.0.0:7451"]`，地图路径同 `config/tracer.json`，`cmd`：`{"goal_enabled": true, "token_file": "@WORK@/cmd_token", "forward": "127.0.0.1:19998"}`。
- `run_sim.sh`：照 `tests/replay/run_replay.sh` 写。渲染到 `$WORK`（默认 `$HOME/nvme/dora-lab/tmp/nav-sim-cmd`），生成 `$WORK/cmd_token`（`umask 077`，24 字节随机 base64），检查没有残留的 `@…@`。`RENDER_ONLY=1` 时只渲染和检查；否则以数据流名 `nav-sim-cmd` 后台启动。停止：`dora stop --name nav-sim-cmd`。
- `check_cmds.sh`：用 `dviz_cmd`（`--connect tcp/127.0.0.1:7451 --robot tracer_sim`）和 `python3 $DORA_NAV/tools/tracer_ctl.py --port 19998`，按设计"离线集成"的表依次执行；每步打印 PASS / FAIL，有 FAIL 就以 1 退出。目标点用 P (0, 1, 0)，再回 P5 (0, 0, 0)；`--wait-goal-state SUCCEEDED --wait-s 60`。

- [ ] **Step 1（实现者）：** 写上面的文件；`RENDER_ONLY=1 bash tests/sim/run_sim.sh` 通过；`bash -n` 两个脚本；渲染出的 YAML 用 `python3 -c 'import yaml,sys; yaml.safe_load(open(sys.argv[1]))'` 能解析，且每个输入只有一个来源。
- [ ] **Step 2（实现者）：提交** `test(nav-bridge): 2D simulation dataflow and command checks for delivery 2`
- [ ] **Step 3（主控）：** `source ~/dora-lab/env.sh`，在实车 dora-nav 照常运行的情况下 `bash tests/sim/run_sim.sh`。Dora 拒绝，或 Orin CPU 持续超过 85% 时，停下来问用户是否先停 dora-nav。
- [ ] **Step 4（主控）：** `bash tests/sim/check_cmds.sh`，期望全部 PASS；记下各步回执。
- [ ] **Step 5（主控）：** 用户在 epyc 上的 Studio 连 `tcp/192.168.1.160:7451`（client），在 2D 图上拖目标、设初始位姿、按 E-STOP，看仿真车和回执；主控截图（不操作鼠标键盘）。结束后 `dora stop --name nav-sim-cmd`。

### Task 9: 部署、实车核对、进度文档（主控执行，每一步按用户的同意）

- [ ] **Step 1：令牌：** Orin 上 `umask 077; mkdir -p ~/.config/dviz; head -c 24 /dev/urandom | base64 > ~/.config/dviz/cmd_token`；用 `scp` 拷到 epyc 同一路径，权限 600。令牌内容不打印。
- [ ] **Step 2：桥节点：** 备份 `opt/nav-bridge` 到 `~/nvme/dora-lab/tmp/nav-bridge-d3-backup`；装新的二进制；`opt/nav-bridge/tracer.json` 加 `"cmd": {"goal_enabled": true}`。
- [ ] **Step 3：重启 dora-nav**（先问用户）：确认 15 个节点在跑，桥节点日志有 `goal/initialpose commands enabled`。
- [ ] **Step 4：epyc：** 更新 `~/Public/dorobot-studio-dviz` 到本分支，构建并重启 Studio，用户连 `tcp/192.168.1.160:7450`。
- [ ] **Step 5：静止核对：**
  - E-STOP → 状态行安全门急停；我在车上 `tracer_ctl.py release`；
  - Disarm → `ok`；
  - 未武装时 `2D Goal Pose` 灰；`dviz_cmd … --cmd goal` 回 `not_armed`；`--cmd arm` 回 `not_allowed`；
  - 在车当前位置附近设一次 `2D Pose Estimate`：重定位接受，定位不跳。
- [ ] **Step 6（用户说 go 后）：** 我在车上 `arm`；Studio 发 P5 → P 约 1 m 的目标，车到点；结束后 disarm。
- [ ] **Step 7（用户说 go 后）：** arm，发目标，行驶中按 Studio `E-STOP` → 车停；之后 `release`，保持撤防。
- [ ] **Step 8（用户说 go 后）：** arm，发目标，行驶中 Studio 点 Disconnect → 车照常到点；disarm。
- [ ] **Step 9：** 用户要的话照交付③录屏；写进度文档 `docs/superpowers/plans/2026-11-24-dviz-p2-commands-progress.md`（中文：各任务提交、测试数字、仿真和实车结果），提交 `docs(dviz): delivery 2 progress`。
