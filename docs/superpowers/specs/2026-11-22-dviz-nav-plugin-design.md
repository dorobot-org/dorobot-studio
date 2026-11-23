# dviz 导航可视化插件（Studio 托管界面，替代 RViz）

日期：2026-11-22

## 用户决定（本次讨论）

- DoRobot Studio 是替代 RViz 和 Gazebo 的界面工具，由它托管界面；dviz 的功能做成 Studio 插件，以后的功能也加在插件里。
- 顺序：**先**把 dviz 做成 Studio 插件，**再**按"DORA_NAV 接入"的要求补功能。
- 3D：**先**由插件拉起一个独立的 Rerun 窗口（选项 A），**以后**再换成画在 Studio 窗口里（选项 B），即选项 C。
- 代码放置：**全部搬进 Studio 仓库**（方案 1）。dviz 仓库冻结，README 指向 Studio。

## 意图

在 Mac 或 Linux 上用 Studio 远程看、远程操作 Tracer Mini 上运行的 Dora 导航栈（DORA_NAV，Dora 1.1，Orin）：
- **看：** 点云、地图、代价地图、路径、目标、定位 / 安全门 / 底盘状态、节点日志和数据流图；
- **做：** 设目标、设初始位姿、软件急停 / 撤防。

dviz 已经有"车上 Dora 节点 → Zenoh → 电脑端 → Rerun"这条链路和一部分面板，但它是独立程序、Dora 版本旧、不认 DORA_NAV 的数据格式（见 superlio_stack `docs/architecture/2026-11-22-autonomy-borrow-evaluation.md` 第 6 节）。

## 目标 / 非目标 / 反目标

**目标**

| 阶段 | 要做的 |
|---|---|
| **P1 插件化** | Studio 里有一个 dviz 插件，功能对齐 dviz-shell 里有用的部分：<br>· 用 Zenoh 接收 dviz 通用协议，并写进插件拉起的 Rerun 0.28 窗口；<br>· 显示列表（按实体开关）；<br>· 节点日志；<br>· 数据流图和节点详情；<br>· 连接状态。<br>用 dviz 自带的仿真示例数据流在本机跑通。 |
| **P2 DORA_NAV 接入** | **车上桥节点**（Dora 1.x）：<br>· 把 DORA_NAV 的输出转成通用协议并降采样；<br>· 发布静态 PCD 地图和 2D 栅格地图；<br>· 发布规划器的代价地图（DORA_NAV 新增输出）；<br>· 接收 Studio 发来的命令，按安全策略转给 `nav_command`。<br>**插件新增：**<br>· 导航状态面板；<br>· 设目标 / 初始位姿工具；<br>· 急停、撤防按钮。 |

**非目标（本设计不做）**
- 画在 Studio 窗口里的原生 3D（选项 B，以后单独设计）。
- 替代 Gazebo 的仿真部分。
- M6 巡检桥。
- 把 ROS1 bag 回放移进插件：dviz 冻结的版本里还能用，需要时再移。
- 改变 DORA_NAV 的导航行为。

**反目标（做出来也算失败）**
- 能从车外武装或解除急停。`arm`、`release` 只能在车上发。
- 桥节点拖慢或阻塞导航数据流：队列必须有界，满了丢最旧的。
- DORA_NAV 硬依赖 Studio：没有桥节点时导航照常；桥节点没人连时只空转。
- 破坏 Studio 现有插件、测试和 `--locked` 构建。

## 约束

- **Studio 侧：**
  - Makepad 固定为 OctoSense-org/makepad `44d8220f`，用 `script_eval!` / `script_from_value` 的写法；
  - 插件契约 `dorobot-studio-plugin` API 版本 1；
  - 构建一律 `--locked`；
  - Zenoh `=1.10.2`（`transport_tcp`）；Rerun `0.28`（Studio 已有的可选依赖）。
- **dviz 界面代码不能直接搬：** dviz 用的是旧的 `live_design!` 写法、官方 Makepad main，所以面板要按 Studio 的写法重写；非界面代码（协议、类型、TF）可以复用。
- **车上桥节点：**
  - 必须用 dora-node-api 1.x：Dora 1.1 守护进程接受 1.0.1，拒绝 dviz 现在的 0.5；Studio 的 `runtime/` 已固定 `=1.0.1`。
  - 在 Orin（aarch64，Rust stable 1.98）上编译。
  - 不进 DORA_NAV 仓库。
- **网络：**
  - 走 Wi-Fi，组播发现可能被阻断，所以要能指定连接地址（Studio 的 `ZenohOptions` 已支持）；
  - 带宽有限，点云必须降采样。
- **DORA_NAV 侧：**
  - `nav_command` 只听本机回环地址（`127.0.0.1:9998`），每条命令都回 `{"ok":...}`；
  - 输出都是 UInt8 字节数组，格式如下：

  | 数据 | 格式 |
  |---|---|
  | 位姿 `pose_fuser/map_base` | Odometry JSON |
  | 路径 `astar_planner/path` | `{"header","goal_id","poses":[{"x","y"}],"error"}` |
  | 目标 `goal_resume/goal` | `{"id","source","x","y","yaw"?}` |
  | 目标状态 `pure_pursuit/goal_status` | `{"id","state","reason"}` |
  | 定位可信度 `loc_trust/status`、重定位状态 `loc_refine/state` | JSON |
  | 安全门状态、取消 `cmd_gate/state`、`cmd_gate/cancel` | JSON |
  | 底盘状态 `tracer/state` | JSON |
  | 点云 `super_lio/cloud_body`、`cloud_world` | XYZI 二进制 `[f64 stamp][u32 N][N×4 f32]` |
  | 激光 `cloud_to_scan/scan` | LaserScan 二进制（28 字节头 + f32 距离） |

  - 地图：`map.yaml` + `map.pgm` 和 `map.pcd`，路径写在 DORA_NAV 的配置里。
- **许可证：** Studio 工作区和 `runtime/` 声明 **MIT**；dviz 是 **Apache-2.0**，有一位外部贡献者（sqw420，1 个提交）。**我的决定：** 搬进来的 crate 保持 Apache-2.0（各自 `Cargo.toml` 写 `license = "Apache-2.0"`），仓库加 `NOTICE` 说明出处，不改许可证；改许可证需要贡献者同意。

## 方案（讨论中已选定 1）

1. **全部搬进 Studio（选定）：**
   - 插件放 `plugins/`；
   - dviz 的非界面代码放 `crates/`；
   - 车上桥节点放 Studio 仓库的原生运行时工作区 `runtime/`（已是 Dora 1.0.1、面向 Linux）；
   - DORA_NAV 只多一个代价地图输出，外加一种可选的接桥方式。
   - **好处：** 协议只有一份，插件和桥节点一起改、一起测；版本由 Studio 统一固定。
2. **dviz 保留为库仓库（未选）：** 每个功能都要跨两个仓库改，版本来回对齐；dviz 的界面部分进不了 Studio，还是要重写。
3. **桥节点放进 DORA_NAV（未选）：** DORA_NAV 会多一个只给 Studio 用的 Zenoh 依赖；协议一改，两个仓库都要改；也违反"工具层不进 DORA_NAV"。

## 设计

### 1. 仓库内的放置（Studio）

| 位置 | 内容 | 来源 |
|---|---|---|
| `crates/dviz-protocol` | 通用协议：消息头 + 二进制载荷、实体路径、日志 / 节点定义 / 活动；P2 新增 `grid2d`、`status`、`cmd` / `cmd_ack` 类型；编码、解码和校验函数 | dviz-core `zenoh_protocol.rs` 和 `types/` |
| `crates/dviz-transform` | TF 树和变换缓冲 | dviz-transform 原样搬入 |
| `plugins/dviz` | 插件本体：面板、Zenoh 接收线程、Rerun 写入 | 按 Studio 写法重写 dviz-shell / dviz-widgets 的有用部分 |
| `runtime/`（新二进制，名字由实现者定，例如 `dorobot-nav-bridge`） | 车上桥节点 | dviz-rerun-bridge 的思路，加上 DORA_NAV 适配 |

- URDF 用 Studio 已有的 `RobotModelLoader`，不搬 dviz-urdf。
- 搬入的代码保留原版权头，在 `CHANGELOG` 或 README 注明来自 dviz 的哪个提交。

### 2. 插件（电脑端）

- **插件描述：**
  - id `dviz`，API 版本 1。
  - P1 一个面板（例如 `dviz.view`）：连接（Zenoh 模式 / 地址、状态）、"打开 3D 窗口"、显示列表、日志、数据流图 / 节点详情。
  - P2 加一个导航面板（例如 `dviz.nav`）：定位、安全门、底盘状态，当前目标和目标状态，设目标 / 设初始位姿，急停、撤防按钮，命令回执。
- **线程模型：** 照 Studio 遥测接收器（`dorobot-studio-core/src/api/telemetry/receiver.rs`）的做法：
  - 后台线程跑同步的 Zenoh 会话，订阅 `dviz/<robot>/**`；
  - 解码后直接写进 Rerun 记录流；
  - 状态、日志、节点信息放进共享快照，界面在 `sync()` 里读，`handle_actions()` 里处理按钮。
- **连接的归属（我的决定，审查后修订）：** Studio 每次隐藏面板都会调 `on_disable`（包括在 `dviz.view` 和 `dviz.nav` 之间切换），而插件工厂不能保存状态。所以连接（Zenoh 会话、接收线程、Rerun 记录流）由**插件 crate 里一个进程级的会话对象**持有，两个面板共用同一个，各自只拿句柄：
  - 切换或离开面板时连接不断，Rerun 窗口照常更新，急停按钮回到面板时仍然有效；
  - 只有"断开"按钮或程序退出才停；
  - `on_disable` 只停界面侧的定时刷新。
  - 不放进 studio-core，保持核心库不变。
- **3D：**
  - 用 Rerun 0.28 的 `spawn`（或连接一个已开着的查看器），一个机器人一个记录；
  - 实体路径沿用 dviz 的 `{prefix}/{entity}` → `{entity}`；
  - 静态内容（地图）用 static 记录；
  - Studio 构建时要打开 Rerun 依赖（目前是可选特性，默认关）：插件声明这个特性。
- **设目标：** P2 先做表单（x、y、朝向，或从地图上已命名的点里选）；在 2D 地图上点选留给选项 B。
- **机器人模型（我的决定）：** P2 在 Rerun 里把 Tracer 画成 0.40 × 0.40 m 的方框加朝向箭头（足迹即可）。`RobotModelLoader` 产出的是 Studio 自己 `RobotView` 用的模型，不是 Rerun 网格，所以真正的 URDF 网格等选项 B 再做；届时 Tracer 的展开 URDF 和网格从 tracer-mini-ros2 的 xacro 生成。
- **构建开关（我的决定）：**
  - 插件始终编译；
  - 写 Rerun 的部分放在一个特性后面（例如 `rerun`），桌面默认构建打开；CI 用的 `--no-default-features` 不编 Rerun，走空实现（照 `crates/dorobot-dora-bridge/src/rerun_stub.rs` 的做法），所以 CI 时间不受影响；
  - Rerun 0.28 查看器是使用前提（`cargo install rerun-cli --version 0.28.x --locked`），写进插件文档；随发布包一起打包以后再说。

### 3. 车上桥节点（Orin，接进 DORA_NAV 的数据流）

- **输入：** 上面约束表里的那些 DORA_NAV 输出，按输入名在配置文件里映射到格式，不写死名字。
- **输出：** 往 Zenoh 发通用协议，话题前缀 `dviz/<robot_id>`，`robot_id` 由配置或环境变量指定，默认 `tracer`。

| 数据 | 处理 | 频率上限 |
|---|---|---|
| 点云 | 体素或抽稀到约 5k 点 | 5 Hz |
| 位姿 | 转 `transform3d` 或位姿 | 20 Hz |
| 路径 | 转 `linestrips3d` | 有新路径时 |
| 代价地图 | `grid2d`，降采样 | 1 Hz |
| 状态 | 安全门、底盘、定位合成一个 `status` | 5 Hz |
| 日志 | 节点名 + 级别 + 文本 | 限流（见下） |

- **静态内容：** 启动时读一次 PCD 地图（抽稀）和栅格地图，之后每 5 s 补发一次，让晚连上的 Studio 也能看到。
- **节点日志和数据流图（我的决定，审查后修订）：** 用 DORA_NAV 各节点**真实的日志**，不是 dviz 那种由桥自己编出来的输入输出活动：
  - 桥节点跟踪 Dora 写在车上的 `out/<uuid>/log_<node>.jsonl`（数据流目录由配置给）；
  - 默认只转 WARN / ERROR，外加少量关键 INFO；按节点限流（例如每节点每秒 ≤ 5 条）；
  - 节点是否活着，由它的日志和输出活动推断，数据流图来自数据流 YAML；
  - 不查询 Dora 协调器（以后需要"重启中 / 失败"这种精确状态时再加）。
- **命令（反向）：**
  - 订阅 `dviz/<robot_id>/cmd`，JSON `{"cmd":..., "token"?, ...}`；
  - **只转发 `goal`、`initialpose`、`estop`、`disarm`**，其余（含 `arm`、`release`）一律拒绝；`goal` / `initialpose` 还要校验令牌（见"安全"）；
  - 转发到 `127.0.0.1:9998`；把 `nav_command` 的回执（或"没有回复 / 被拒绝"）发到 `dviz/<robot_id>/cmd_ack`；
  - 每条命令记日志。
- **不拖累导航：**
  - 处理在 Dora 事件循环外的有界队列里做，满了丢最旧的；
  - Zenoh 发不出去不阻塞；
  - 没有任何订阅者时只做最少的工作。
- **接进数据流：** DORA_NAV 的 `apps/tracer/tracer_nav.yml` 不直接写 Studio 二进制的路径。做法（实现者定细节）：
  - 一个叠加片段（例如 `apps/tracer/viz_bridge.yml.in`，路径由环境变量给）；
  - `autostart.sh` 在设置了例如 `NAV_VIZ_BRIDGE=/path/to/bridge` 时，把它并进数据流再启动；
  - 没设置时和现在完全一样。

### 4. DORA_NAV 的改动（核心导航，单独小计划）

- **规划器新输出 `costmap`：**
  - 内容：二进制头（宽、高、分辨率、原点、时间戳）+ u8 代价（0–255，Nav2 语义）；
  - 降采样（例如 ×2），限频 ≤ 1 Hz；
  - 先写测试。
- **叠加接桥机制：** 见上一节，`apps/tracer`。

### 5. 安全

- **车外能发的只有：** 设目标、设初始位姿、急停、撤防。桥节点拒绝其他命令；插件也不提供武装、解除急停的按钮。
- **谁能发命令（用户 2026-11-22 决定：共享令牌）：**
  - **默认关闭：** 桥节点的命令通道默认关闭，要在车上显式打开（例如环境变量或配置项）。
  - **令牌：** `goal`、`initialpose` 必须带令牌。令牌放在车上和操作员机器上各一份文件里（例如 `~/.config/dviz/cmd_token`，权限 600，路径由实现者定）；令牌不对或缺失就拒绝，并在回执和日志里写明。
  - **急停、撤防不需要令牌：** 任何连上的 Studio 都能发，因为让车停下总是被允许的。
  - **已知局限：** 令牌在 Wi-Fi 上明文传输，只防误操作和随手发命令，防不了抓包。离开实验室网络之前，改用 Zenoh 双向 TLS（证书）。
- **Wi-Fi 上的急停不是安全装置：** 遥控器切 RC 仍是硬件急停；插件界面要写明这一点。
- **`nav_command` 自己的规则照旧：** 只听回环地址、急停也撤防、武装需要人在车上用 `tracer_ctl`。
- **实车测试：** 每一次动车都要用户说 go。

## 测试

- **单元测试：**
  - **协议：** 编解码往返、坏数据被拒。
  - **桥节点：** 每种 DORA_NAV 格式的解析、降采样上限、频率限制、命令过滤（`arm` / `release` 被拒）、回执转发。
  - **规划器：** 代价地图编码。
- **插件：** 照 Studio 惯例：
  - 注册表测试（id 唯一、API 版本）；
  - 每次创建实例互相独立；
  - 真实按钮路由；
  - 停用时线程退出。
- **P1 的数据源（我的决定，审查后修订）：** dviz 的示例数据流要外部的 dora-examples 和 Dora 0.5 桥，这里用不了。所以 P1 用一个**不依赖 Dora 的合成发布器**（开发工具，复用 dviz `simulation.rs`），按通用协议往 Zenoh 发数据。P1 因此完全不需要 Dora。
- **离线集成（不动车；我的决定，审查后修订）：** 测试用的数据流放在 **Studio 仓库**（测试资源），用环境变量 `DORA_NAV` 指向 DORA_NAV 的构建；DORA_NAV 不为测试改动。两个数据流：
  - **(a) 回放：** 同 M5 离线演练，用 `apps/tracer/tracer_nav.yml` 的节点，雷达换成录包回放，不带底盘节点，加桥节点。这样有真实的点云、位姿、定位可信度、规划器输出；
  - **(b) 2D 仿真加命令入口：** 基于 `apps/nav_sim2d.yml`，目标由 `navcmd` 提供（`sim_mission` 只提供可信度和重定位状态），加桥节点。

  再另起一个无界面的 Zenoh 订阅者，核对：
  - 话题齐全，频率和点数在上限内（用 (a)）；
  - 命令能转到 `nav_command`，`arm` / `release` 被拒，没有令牌或令牌错误的 `goal` 被拒，不带令牌的 `estop` 被接受；
  - 从 Studio 设目标，仿真车到点（用 (b)）。
- **端到端：**
  - Studio 在 Linux（Orin 桌面或另一台机器）和 Mac 上各连一次，能看到地图、点云、路径、状态；
  - 在仿真里从 Studio 设目标，车到点。
- **实车（每步用户 go）：**
  - 从 Studio 设目标，车到点；
  - 行驶中从 Studio 急停；
  - 断网时导航不受影响。

## 阶段和工作量（粗估）

| 阶段 | 内容 | 工作量 |
|---|---|---|
| P1 | 搬入协议 / TF crate，插件面板按 Studio 写法重写，进程级会话 + Zenoh 接收线程 + Rerun 写入，合成发布器，插件测试 | 5.5–7.5 人日 |
| P2a | DORA_NAV 规划器代价地图输出 + 接桥叠加机制 | 1–1.5 人日 |
| P2b | 桥节点：DORA_NAV 适配、降采样、静态地图、命令反向通道（含令牌）、真实节点日志，加单元测试；Studio 里的两个测试数据流和离线集成 | 6.5–8 人日 |
| P2c | 插件导航面板：状态、设目标、急停 / 撤防，端到端和实车 | 2–3 人日 |

每个阶段走 计划 → TDD → 审查 → 本地提交；推送和 PR 要用户同意。

## 留给实现者

- crate、二进制、面板的确切名字，界面布局和文案（中英文按 Studio 的 i18n）。
- 实体路径命名、点云降采样算法（体素或步长）。
- 叠加接桥的具体写法。
- 代价地图的二进制布局细节。
- 日志措辞。
