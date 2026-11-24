# dviz P2 交付②：在 Studio 里设初始位姿、发目标、急停 / 撤防 设计

日期：2026-11-24。上一份设计：`docs/superpowers/specs/2026-11-22-dviz-p2-nav-view-design.md`（下称"P2 设计"），本文件细化其中的交付②（§3"2D 工具""按钮"、§5"命令"），冲突处以本文件为准。交付①（看）和③（代价地图、RViz 外观）已完成并在实车验收。

## 用户决定

- **沿用 P2 设计已定的：**
  - 在 Studio 的 2D 地图上用 RViz 式工具设初始位姿、发目标（按下定位置、拖动定朝向、松开发送）；
  - "急停""撤防"只要桥节点在跑就转发，不要令牌，不受开关、限流影响；
  - 设初始位姿、发目标要车上显式打开，还要共享令牌（明文，只防误操作，防不了抓包）；
  - `arm`、`release` 车外一律禁止，界面上也没有这两个按钮。
- **2026-11-24（方案 A）：车没武装或在急停时，桥节点拒收目标**，回执写明原因，Studio 的目标工具同时变灰。设初始位姿不受这条限制（它通常在武装前做）。
- **2026-11-24：命令走 Zenoh 请求 / 应答**（Studio `get`，桥节点 `queryable`），不再用 P2 设计里的"发布到 `cmd`、回执发到 `cmd_ack` 话题"。

## 意图

操作员在 epyc 上的 Studio 里完成 RViz 里那套操作：看到定位偏了，在图上拖一下设初始位姿；点一下、拖一下发目标；出事一键急停或撤防。每条命令都能看到结果：已送达、被拒（为什么）、没送达。

## 现状（交付③之后）

- **协议 `crates/dviz-protocol`：** 没有命令类型。`NavStatus` 已有 `armed`、`estop`（来自 `cmd_gate/state`）、`stale`（各部分 2 s 没更新）和 `cmd_goal_enabled`（交付②预留，现在恒为 `false`）。
- **桥节点 `runtime/nav-bridge`：**
  - 只发布，不收任何东西；话题键固定为 `dviz/<robot_id>/<实体>`（`publish.rs:65`）；
  - Zenoh 会话在 `zenoh_publisher()`（`publish.rs`）里打开（peer，监听 `tcp/0.0.0.0:7450`，关组播），归 worker 的发布器所有，worker 线程结束时会话随之关闭；
  - 已经接收 `cmd_gate/state`（输入 `gate_state`，Dora `queue_size: 2`），但它和其他输入一起进 64 条的共用收件箱（满了丢最旧的），只有 worker 能看到；worker 死后所有输入都被丢掉（`run.rs:124`）；
  - 配置坏了或 Zenoh 打不开时走 `Bridge::disabled()`：没有 Zenoh，只吃掉 Dora 事件。
- **Studio 插件 `plugins/dviz`：**
  - `session.rs` 的会话线程只订阅 `<prefix>/**`；连接设置里只有前缀，没有机器人编号；导航视图在解析话题时丢掉了机器人编号那一段（`nav_scene.rs` `nav_layer`）；
  - Navigation 面板（`nav_panel.rs`）：工具栏 `Fit map`、`Follow robot`，状态行，2D 地图（`map2d.rs`：左键拖动平移、滚轮缩放），图例，"Wi-Fi e-stop is not a safety device…"声明；
  - 面板文案是英文，没接 Studio 的 i18n。
- **车上 DORA_NAV：**
  - `nav_command`（`modules/interface/nav_command/nav_command.py`）只听 `127.0.0.1:9998`（UDP，端口可由 `NAV_COMMAND_PORT` 改），每 100 ms 的 `tick` 处理一次，回 `{"ok":bool,"outputs":[…]}`。`ok:true` 只表示命令已送进导航数据流。
  - 它接受 `goal {x,y,yaw?,id?}`（没有 `yaw` = 不管末端朝向）、`initialpose {x,y,yaw}`、`estop`、`release`、`arm`、`disarm`。
  - `cmd_gate`：急停锁住并撤防；`release` 后仍是撤防，要再 `arm`；急停期间的 `arm` 不理。
  - `cmd_gate` 的 `cancel` 只在"武装 → 撤防"跳变时发（下一个 tick 一次，仍撤防时 0.2 s 后补一次），收到的节点忘掉目标；**已经撤防时再撤防或急停，不再发**（`cmd_gate_dora/README.md` 12–17 行）。
  - `cmd_gate/state` 在变化时发，另外每秒发一次。
  - **为什么要方案 A：** 撤防状态下到达的目标永远不会被取消。`pure_pursuit` 要 15 s 没移动 0.3 m 才放弃（`progress_movement_time_allowance`）；这 15 s 里有人在车上武装，车会马上朝这个目标开。
  - `tools/tracer_ctl.py` 在车上直接发 UDP 到 9998，和桥节点并存。

## 目标 / 非目标 / 反目标

**目标**
- Navigation 工具栏加两个工具 `2D Pose Estimate`、`2D Goal Pose`，两个按钮 `E-STOP`、`Disarm`。
- 工具：在图上按下定位置、拖动定朝向、松开发送；拖动时有预览箭头；发送后工具自动退出；Esc 或再点按钮取消。
- 每条命令的结果显示在面板底部：已送达 / 被拒（原因）/ 没送达（超时、未连接）。
- 工具不可用时变灰，并写明原因。
- 目标的进展照旧看状态行里的目标状态；发出的目标编号就是状态行里的目标编号。

**非目标**
- 车外武装、解除急停。
- 在 Rerun 窗口里点选。
- 多台 Studio 之间互相看到对方发的命令（车状态的变化照样在状态行里）。
- Zenoh TLS（离开实验室网络前再做，P1 设计已记）。
- 路点序列、巡检任务。
- 改 DORA_NAV 的节点。
- 急停的键盘快捷键（我的决定：避免误触；以后需要再加）。

**反目标（做出来也算失败）**
- 能从车外武装或解除急停。
- 急停、撤防被令牌、开关、限流、目标规则、状态过期，或者桥节点 worker 线程挂掉挡住。
- 撤防或急停时，桥节点接受了目标（方案 A）。
- 命令处理阻塞 Dora 事件循环或数据发布；Studio 界面线程等网络。
- 令牌出现在日志、回执或界面里。
- 不设桥节点时 DORA_NAV 和现在不同；破坏 Studio 现有插件、测试和 `--locked` 构建。

## 约束

- P2 设计"约束"一节继续有效：
  - Zenoh `=1.10.2`，只开 `transport_tcp`；
  - Rerun 0.28；Makepad 固定版本；构建一律 `--locked`；
  - 桥节点是独立工作区，dora-node-api `=1.1.0`。
- 车上 dora-nav 在跑时，占着 UDP 9998 和 Zenoh 7450，离线测试要换端口。
- 实车：每次动车前用户先说 go；重启 dora-nav、往车上装东西都先问。

## 方案（已选 2）

1. **发布 / 订阅 + 回执话题（不选）：** 车收不到时 Studio 看不出来，只能自己计时；回执发给所有 Studio，要按编号认领。
2. **请求 / 应答（选定）：** Studio 对 `dviz/<robot_id>/cmd` 发查询，桥节点只回给发查询的那台 Studio；没有回复就是没送达，由查询超时报出。代价：别的 Studio 看不到这条命令。
3. **桥节点直接给 `goal_resume` / `cmd_gate` 发 Dora 消息，绕开 nav_command（不选）：** 好几个 C++ 节点要加新输入，DORA_NAV 改动大，也违背"不设桥节点一切照旧"。

## 设计

### 1. 协议（`crates/dviz-protocol`）

- **键：** `<prefix>/<robot_id>/cmd`（默认 `dviz/tracer/cmd`）。查询不是样本，不影响 Studio 现有的 `<prefix>/**` 订阅。
- **请求 `CmdRequest`**（查询的 payload，JSON，≤ 1 KiB）：
  - 格式：`{"cmd","id","token"?,"x"?,"y"?,"yaw"?}`；
  - `id`：1–64 个 `[A-Za-z0-9_-]` 字符，由 Studio 生成；
  - `goal`：`x`、`y` 必有且有限，`yaw` 可省；
  - `initialpose`：`x`、`y`、`yaw` 必有且有限；
  - `estop`、`disarm`：只有 `cmd`、`id`；
  - 其他 `cmd` 值（`arm`、`release`、未知）协议层照样解出来，由桥节点回 `not_allowed`。
- **回复 `CmdAck`**（JSON）：`{"id","cmd","result","reason"}`。
  - `result` 是下表中的一个代码；
  - `reason` 是一句给人看的英文，不含令牌。

  | `result` | 含义 |
  |---|---|
  | `ok` | nav_command 回了 `ok:true`：命令已送进导航数据流 |
  | `not_allowed` | `arm`、`release` 或未知命令 |
  | `disabled` | 车上没开放 `goal` / `initialpose`（开关关，或车上令牌不可用） |
  | `bad_token` | 令牌缺失或不对 |
  | `not_armed` | 目标：安全门撤防中 |
  | `estop_active` | 目标：急停中 |
  | `gate_unknown` | 目标：2 s 内没收到安全门状态 |
  | `rate_limited` | `goal` + `initialpose` 超过每秒 2 条 |
  | `bad_request` | 解析失败、缺字段、数不是有限值、超长、`id` 不合法 |
  | `nav_rejected` | nav_command 回了 `ok:false` |
  | `no_reply` | nav_command 在超时内没回（节点挂了或正在重启） |

  Studio 自己还会产生两种（不上线路）：`not_connected`（没连上，没发出去）、`timeout`（查询超时，没有回复）。
- **`NavStatus.cmd_goal_enabled`：** 语义改为"车上已开放 `goal` / `initialpose`"，即配置开关打开且车上令牌可用。旧桥节点不带这个字段时仍按 `false` 解。
- 校验、往返测试、坏数据被拒不崩溃，照 P1 的做法。

### 2. 桥节点（`runtime/nav-bridge`）

- **配置新段 `cmd`（全部有默认值，仓库默认关闭）：**

  ```json
  "cmd": {
    "goal_enabled": false,
    "token_file": "~/.config/dviz/cmd_token",
    "forward": "127.0.0.1:9998",
    "reply_timeout_ms": 1000,
    "max_goal_per_s": 2
  }
  ```

  `~` 展开为 `$HOME`。`goal_enabled` 只管 `goal` / `initialpose`；`estop` / `disarm` 不看它。
- **会话归属要改（审查发现）：** 现在 Zenoh 会话归 worker 的发布器所有，worker 结束会话就关。改为在桥节点启动时打开会话，由桥节点本身持有到进程结束；worker 的发布器和命令线程各拿一个克隆。worker 挂了，会话和命令通道都还在。
- **命令线程：**
  - 在这个会话上声明 `queryable`，键 `dviz/<robot_id>/cmd`（和发布用同一个 `robot_id`）；
  - 查询进有界通道，由单独的命令线程处理，不在 Zenoh 回调里做 I/O；
  - 和 worker、Dora 事件循环互不依赖：worker 挂了，命令线程照样转发急停和撤防。
- **每条请求的处理顺序：**
  1. 解析，失败回 `bad_request`。
  2. `estop`、`disarm`：直接转发，不看令牌、开关、限流和门状态。
  3. `arm`、`release`、其他：回 `not_allowed`。
  4. `goal`、`initialpose`：
     - 开关关或车上令牌不可用 → `disabled`；
     - 令牌不对 → `bad_token`；
     - 超限 → `rate_limited`。`goal` 和 `initialpose` 共用一个桶，每秒 2 条，突发 2；
     - 只对 `goal`：门状态超过 2 s 没更新或从未收到 → `gate_unknown`；急停中 → `estop_active`；撤防中 → `not_armed`。
  5. 转发：UDP 发到 `forward`。
     - 内容是 nav_command 的格式 `{"cmd","x","y","yaw"?,"id"?}`。`goal` 的 `id` 用请求 `id`，所以状态行里的目标编号就是 Studio 发的编号。
     - 等 `reply_timeout_ms`：`ok:true` → `ok`，`ok:false` → `nav_rejected`，超时 → `no_reply`。
  6. 回复查询，写一行日志：命令、`id`、结果、坐标，不写令牌。
- **急停不排队：** 一条 `goal` 正在等 nav_command 回复时来了急停，急停不能等那条超时。要求：急停 / 撤防从桥节点收到到发出 UDP ≤ 50 ms。怎么做由实现者定，比如急停用单独的套接字立即发，或者转发不串行。
- **门状态（审查后定，我的决定）：**
  - Dora 事件循环收到 `gate_state` 输入时，除了照旧放进收件箱，还把这份字节**直接**存进一个"最新值"槽位（带收到时刻），给命令线程用。不经过会丢消息的共用收件箱；worker 死了也照样更新。解析放在命令线程里，事件循环只做一次拷贝，不阻塞。
  - 命令线程判断目标时读这个槽位：从未收到、超过 2 s，或者解析失败，都回 `gate_unknown`。`cmd_gate/state` 每秒至少发一次，正常时不会误拒。
  - 要求：拿不到新状态时只会拒目标，绝不按旧状态放行；有单元测试证明撤防消息一到，下一条目标就被拒。
- **已知局限（我的决定，写进文档）：** 桥节点检查"已武装"到 nav_command 把目标送进导航，中间有约 0.1–0.2 s（nav_command 每 100 ms 处理一次）。恰好在这段时间里撤防，目标仍会留下，直到 15 s 后放弃。今天在车上用 `tracer_ctl` 发目标也有同样的空隙。彻底消掉要改 DORA_NAV（例如让 `goal_resume` 撤防时不收目标），那会改变导航行为，不在本交付范围内。
- **令牌：**
  - 启动时读一次 `token_file`，去掉首尾空白；
  - 以下情况都当"车上令牌不可用"：文件不存在、为空、权限比 600 宽（组或其他人可读写）。这时 `goal` / `initialpose` 关闭，日志写一次原因；
  - 用常数时间比较；
  - 改令牌要重启桥节点。
- **状态：** `cmd_goal_enabled = goal_enabled && 车上令牌可用`。
- **`Bridge::disabled()`：** 没有 Zenoh，也就没有命令通道；Studio 那边会看到 `timeout` 或 `not_connected`。现状如此，日志已有说明。

### 3. Studio 插件（`plugins/dviz`）

#### 3.1 会话

- 会话线程打开 Zenoh 后，把会话句柄放进共享状态（`zenoh::Session` 可以克隆）；断开时清掉。
- **命令发给哪台车（审查后定，我的决定）：**
  - Studio 从收到的话题 `<prefix>/<robot_id>/…` 里记下这次连接见过的机器人编号，断开时清空。
  - `E-STOP`、`Disarm` 发给这次连接见过的**每一个**编号的 `<prefix>/<robot_id>/cmd`。让车停总是允许的；没有用通配符 `dviz/*/cmd`，所以不会停掉 Studio 从没见过的车。
  - `2D Pose Estimate`、`2D Goal Pose` 只在正好见过一个编号时可用；见过多个时两个工具变灰，原因写"more than one robot on this connection"。
  - 还没收到任何数据时，点 `E-STOP` 立刻显示 `not_connected`（原因写"no robot seen yet"）。连上车后第一批数据在一秒内就到。
  - 仿真用的桥节点用不同的编号（`tracer_sim`），和实车 `tracer` 分开。
  - 不加"机器人编号"设置项。
- **发命令接口**（名字由实现者定，例如 `DvizSession::send_command`）：
  - 立即返回，不等网络；
  - 没连上时，直接记一条 `not_connected` 回执；
  - 用查询的回调把回复写回共享状态；3 s 内没有回复，记 `timeout`（桥节点等 nav_command 最多 1 s，加余量）。
- **急停 / 撤防的查询：** 用 Zenoh 允许的最高优先级，拥塞时不丢（阻塞式拥塞控制）。其他命令用默认值。
- **令牌：**
  - 每次连接时读 `~/.config/dviz/cmd_token`，只放在内存里；
  - 读不到或为空时，两个工具变灰。
- **请求 `id`：** 同一个 Studio 进程内唯一，≤ 64 字符，短一些好在状态行里认（例如 `s3`、`s4`）。格式由实现者定。
- **快照里多带：** 最近一条命令（命令、`id`、结果、原因、时刻）、是否有命令在等回复、本机有没有令牌。

#### 3.2 面板

```
┌ Navigation ─────────────────────────────────────────────────────────────────────┐
│ [Fit map] [Follow robot] │ [2D Pose Estimate] [2D Goal Pose] │ [E-STOP] [Disarm] │
│ Loc ✔ trusted · Gate armed · Chassis CAN · Goal s3 ACTIVE · 0.20 m/s             │
│ ┌ map ─────────────────────────────────────────────────┐ ┌ Layers ─────────────┐ │
│ │   ↗ 拖动时的预览箭头                                  │ │ …                   │ │
│ └──────────────────────────────────────────────────────┘ └─────────────────────┘ │
│ Last command: goal s3 → ok (sent to nav_command)                       07:41:12  │
│ 2D Goal Pose off: robot not armed (arm it on the robot)                           │
└──────────────────────────────────────────────────────────────────────────────────┘
```

- **文案：** 和现在的 dviz 面板一样用英文，本次不接 i18n。
- **按钮：**
  - `E-STOP` 用 dorobot-ux 的 `HotBtn`（红色），`Disarm` 用醒目的次级样式（例如 `MiniHot`）；
  - 两个按钮永远可点，没有确认框；没连上时点了，立刻显示 `not_connected`。
- **工具按钮：** 用 `SegmentButton`，工具激活时显示为选中。
- **变灰规则**（按下表顺序，第一条命中的原因显示在回执下面一行）：

  | 条件 | 2D Pose Estimate | 2D Goal Pose | 原因文案（大意） |
  |---|---|---|---|
  | 未连接，或还没见过机器人 | 灰 | 灰 | not connected / no robot seen yet |
  | 见过不止一个机器人编号 | 灰 | 灰 | more than one robot on this connection |
  | 本机没有令牌 | 灰 | 灰 | no token (`~/.config/dviz/cmd_token`) |
  | 导航状态过期（> 2 s） | 灰 | 灰 | nav status stale |
  | `cmd_goal_enabled` 为假 | 灰 | 灰 | goal / pose commands not enabled on the robot |
  | 安全门部分 stale | — | 灰 | gate state unknown |
  | `estop` | — | 灰 | e-stop latched (release it on the robot) |
  | 未武装 | — | 灰 | robot not armed (arm it on the robot) |

- 工具激活或拖到一半时条件变了（例如车被撤防），工具直接退出，这次拖动作废。
- 桥节点会再做一遍同样的检查；Studio 的变灰只是提示，不是保护。

#### 3.3 2D 工具（`map2d.rs`）

- **状态：** 空闲 → 工具激活（种类）→ 拖动中（种类，起点的地图坐标）→ 发送 → 空闲。
- **工具激活时：**
  - 左键拖动用来定朝向，不平移；
  - 滚轮缩放和跟随车照常；
  - 起点存地图坐标，视图动了起点也不漂；
  - 光标用十字。
- **预览：** 起点画一个点，拖动时画箭头。
  - `2D Goal Pose` 用目标图层的颜色（现为橙色）；
  - `2D Pose Estimate` 用和实际轨迹（绿色）分得开的颜色，具体色值实现者定。
- **松开时：**
  - 拖动 ≥ 10 像素：`yaw` = 拖动方向在地图系里的角度（屏幕 y 向下，要翻转）。
  - 拖动 < 10 像素（我的决定）：
    - `2D Goal Pose` 发不带 `yaw` 的目标，车到点后不调整朝向；
    - `2D Pose Estimate` 不发送，提示"drag to set the heading"，工具保持激活。初始位姿的朝向对定位很关键，不猜。
- **发送后：** 工具退出（RViz 习惯）。
- **取消：** Esc 或再点一次工具按钮。按下地图时地图拿到键盘焦点。
- **地图外：** 点在栅格范围外也照发，由导航自己处理（规划失败时目标状态会写 ABORTED 和原因）。我的决定：Studio 端不做范围限制。

#### 3.4 回执和进展

- **回执行：** `Last command: <cmd> <id> → <result>: <reason>`，带时刻。
  - `ok` 用正常色，其他结果用醒目的红色，跟主题走；
  - 有命令在等回复时，显示"sending…"。
- **目标进展：** 照旧在状态行。发出的目标 `id` 就是状态行里的目标编号，用户能对上。

### 4. DORA_NAV

- 不改代码。nav_command 和 cmd_gate 的现有行为够用。
- 测试数据流放在 Studio 仓库（见"测试"），不放进 DORA_NAV。

### 5. 部署（实车；每一步按惯例先问）

1. 在 Orin 上生成令牌 `~/.config/dviz/cmd_token`（权限 600），复制到 epyc 同一路径（权限 600）。
2. 车上 `opt/nav-bridge/tracer.json` 加 `"cmd": {"goal_enabled": true}`；仓库里的默认值仍是 `false`。
3. 装新桥节点（先备份交付③的版本），重启 dora-nav（先问）。
4. epyc 更新 Studio 并重启。
5. 更新 `docs/DVIZ.md`（第 135 行那句"还没有"）和桥节点 README：命令通道、令牌、怎么打开。

## 测试

**单元测试（先写失败的测试）**

- **协议：**
  - `CmdRequest` / `CmdAck` 往返；
  - 坏请求被拒：超长、非有限数、缺字段、`id` 非法；
  - 未知 `cmd` 能解出来；
  - 旧 `NavStatus` 兼容。
- **桥节点：**
  - 处理顺序用表格驱动测试：开关关、无令牌、限流已满、急停中、门状态未知时，`estop` / `disarm` 都照样转发；
  - `arm` / `release` 被拒；
  - 方案 A 的三种拒绝；`initialpose` 不受武装限制；
  - 门状态槽位：撤防消息一进事件循环，下一条目标就被拒；worker 已死时槽位照样更新、急停照样转发；
  - 会话在 worker 结束后仍然开着，`queryable` 仍能回复；
  - 令牌文件缺失、为空、权限过宽；
  - 限流；
  - 转发用测试里的假 nav_command（127.0.0.1 随机端口），覆盖 `ok:true`、`ok:false`、不回复（`no_reply`）；
  - 急停不排队：假 nav_command 对 `goal` 不回复，急停仍在 50 ms 内发出；
  - 日志和回执里没有令牌。
- **Zenoh 回环**（照 `tests/zenoh_loop.rs`）：
  - `get` → `queryable` → 假 nav_command → 回复；
  - 没有桥节点时，Studio 侧报 `timeout`。
- **插件：**
  - 工具状态机；
  - 拖动换算：屏幕 → 地图、`yaw` 翻转、短拖动规则；
  - 变灰规则表；
  - 机器人编号：从话题里记下、断开清空；急停发给见过的每个编号；见过多个编号时工具变灰；
  - 回执文案；
  - 发命令：未连接、超时、成功（回环 Zenoh 加假 `queryable`）。

**离线集成（Orin，不动车；dora 由主会话运行，子代理不跑 dora）**

- 新测试数据流放在 `runtime/nav-bridge/tests/sim/`，从 DORA_NAV 的 `apps/nav_sim2d.yml` 改出来（审查发现：Dora 的一个输入只能有一个来源，所以是改接线，不是加节点）：
  - `goal_resume` 的 `goal`、`cmd_gate` 的 `arm` / `estop` 改接 nav_command 的同名输出；
  - `sim_mission` 只再提供可信度和重定位状态。它原来到 `arm_at_s` 就武装、还会发目标，这两个输出不再接给任何节点，用哪份场景文件由实现者定；
  - nav_command 用 `NAV_COMMAND_PORT=19998`；
  - 加桥节点：`robot_id` 为 `tracer_sim`，监听 `tcp/0.0.0.0:7451`，转发到 `127.0.0.1:19998`，`goal_enabled` 为真，用测试令牌；
  - 端口不和实车 dora-nav 冲突，可以同时跑；Dora 不允许或 CPU 不够时，先问用户停 dora-nav。
- **初始位姿在仿真里只能核对回执（我的决定）：** 仿真里没有节点消费 `initialpose`（没有 hdl / loc_refine，`sim2d` 只吃 `cmd_vel`），设了也看不到车跳。仿真只核对回执是 `ok`，真实效果留到实车静止测试。
- 用一个无界面的探针脚本（给 `dviz_probe` 加命令模式，或新写一个 example），依次核对：

  | 步骤 | 期望 |
  |---|---|
  | 未武装时发 `goal` | `not_armed` |
  | 本地 UDP 发 `arm`（代表"车上的人"） | 安全门武装 |
  | 发 `goal` | `ok`，仿真车到点，目标状态 `SUCCEEDED` |
  | 再发 `goal`，行驶中发 `estop` | `ok`，仿真车停 |
  | 发 `arm` / `release` | `not_allowed` |
  | 不带令牌发 `goal` | `bad_token` |
  | 不带令牌发 `estop` | `ok` |

**epyc 端到端（不动车）**

- Studio 连仿真桥节点 `tcp/192.168.1.160:7451`。用户在 2D 图上拖目标、设初始位姿、按 E-STOP，看仿真车动没动、回执对不对。

**实车（每次动车前用户先说 go）**

1. **静止：**
   - Studio `E-STOP` → 安全门急停；之后由我在车上用 `tracer_ctl.py release`，车仍撤防；
   - `Disarm` → `ok`；
   - 未武装时 `2D Goal Pose` 是灰的；用探针发目标，桥节点回 `not_armed`；
   - 在车当前位置附近设一次 `2D Pose Estimate`：重定位接受，定位不跳。
2. **go 后：** 我在车上 `arm`，Studio 发约 1 m 的目标，车到点。
3. **go 后：** 再发目标，行驶中按 Studio `E-STOP` → 车停；之后在车上 `release`，保持撤防。
4. **go 后：** 行驶中 Studio 断开连接 → 车照常到点。
5. 用户需要的话，照交付③的方式录屏。

## 工作量

4–5 人日，和 P2 设计的估计相同。每个任务走：先写测试 → 审查 → 本地提交；推送和 PR 要用户同意。

## 留给实现者

- 类型、函数、配置键、文件的确切名字；请求 `id` 的格式。
- "最新门状态"槽位的具体数据结构；急停不排队怎么实现（满足 ≤ 50 ms）。
- `2D Pose Estimate` 预览颜色、预览箭头的长度和粗细、回执行的样式。
- 仿真数据流和探针的具体写法。
