# 3D visualization（dviz 插件）

插件 `dorobot.dviz`（crate `plugins/dviz`，`dorobot-plugin-dviz`）把机器人通过 Zenoh 发布的 dviz 消息显示出来：3D 内容（点云、位姿、路径、箭头、方框、文字、标量）送进独立的 Rerun 查看器，日志和数据流图显示在 Studio 的“Visualization”面板里。协议和 Rerun 写入逻辑来自 dviz 仓库（见仓库根的 `NOTICE`），本插件按 Apache-2.0 分发。

## 前提

- Rerun 0.28 查看器在 `PATH` 里：`cargo install rerun-cli --version 0.28.x --locked`（命令行名为 `rerun`）。没有它，或者 `rerun --version` 的主、次版本号不是 0.28，“Open 3D window”都会在状态栏报错（版本不对时写出查看器和 Studio 内置 SDK 两边的版本），不会启动它；其余功能照常。
- 构建带 Rerun：`dorobot-flex` 的默认特性已包含 `rerun`；`--no-default-features` 构建不含 Rerun，“Open 3D window”会提示“This build does not include Rerun”。
- 3D 内容在独立的 Rerun 窗口里，不嵌在 Studio 内。

## 连接

在侧栏进入“3D visualization”，面板“Visualization”里有三个输入：

| 输入 | 含义 |
|---|---|
| Mode | `peer`（默认）或 `client` |
| Endpoint | Zenoh 端点，如 `tcp/192.168.1.186:7447`；多个用逗号或空格分隔 |
| Prefix | 话题前缀，默认 `dviz`；订阅 `<prefix>/**`，Rerun 实体路径 = 话题去掉 `<prefix>/` |

- **同一局域网**：Mode 填 `peer`，Endpoint 留空，用组播自动发现。
- **跨 Wi-Fi / 组播不通**：Endpoint 填机器人上监听的地址；`client` 模式必须填 Endpoint，`peer` 模式填了就直连该地址。

点“Connect”建立会话，“Disconnect”断开，“Open 3D window”打开 Rerun。切到别的页面再回来，连接和已收到的数据保留（会话是进程内全局的）。这些按钮都立即返回，界面不会等 Zenoh 或 Rerun。

状态栏第一行是连接状态和计数：

- **Connected**：至少和一个 Zenoh peer 或 router 建立了连接。只是会话打开、没有对端，不算已连接。
- **Listening, no peer found yet**：Endpoint 留空时，会话已打开、正在用组播找对端，还没找到。这不是错误；2 秒后仍没有对端，下一行提示 “Hint: no dviz peer found by multicast; enter the robot's endpoint”，这时在 Endpoint 填机器人上监听的地址再点“Connect”。
- **Not connected**：没有会话，或填了 Endpoint 但还没连上。

填了 Endpoint 却连不上时，约 2 秒内出现带端点名的 “Error:” 行，后台继续重试：`client` 模式是 1 秒连接超时后报错；`peer` 模式是打开会话（最多等 1 秒连接超时）后再等 1 秒仍无连接才报错。连上后错误行消失。

“3D window:” 行显示最近一次“Open 3D window”的结果：

- `starting…`：正在后台启动。Studio 先跑 `rerun --version` 核对版本，再启动查看器（如果 127.0.0.1:9876 上已有查看器在监听，就直接用它），最多等 10 秒，直到它的端口能连上才把 3D 数据送过去。启动中再点按钮不起作用。
- `open (Rerun 0.28.x)`：已连上，3D 数据送往这个查看器。
- 其他文字：失败原因，例如没装 `rerun`、版本不对、查看器一启动就退出、10 秒内没打开端口。失败时不会建立到它的数据通道，日志和节点照常更新；可以修好后再点一次。

面板内容：

- **Displays**：每个收到的实体一行，带开关和消息计数；关掉后该实体不再送进 Rerun。
- **Logs**：最近的 12 条日志，最新的在最上面；每条只占一行，超过 160 个字符的截断并以“…”结尾。“WARN and above”只看警告及以上。
- **Nodes**：节点“List”或“Graph”（数据流图）。每个节点显示状态和 “seen at t=12.3 s”：t 是发送方的时钟（数据流启动以来的秒数），不是“多少秒以前”。图里点击节点显示它的输入输出和上下游。

## 合成发布器

没有真机时用合成发布器试：

```sh
# 终端 1：发布器监听 7447
cargo run --locked -p dorobot-plugin-dviz --example fake_publisher -- \
  --listen tcp/0.0.0.0:7447 [--connect tcp/host:7447] [--prefix dviz] [--hz 10]
```

参数：`--listen` / `--connect` 可重复；`--prefix` 默认 `dviz`；`--hz` 默认 10。Ctrl-C 结束。Studio 里 Mode 填 `client`，Endpoint 填 `tcp/<发布器所在机器的 IP>:7447`（同一台机器用 `tcp/127.0.0.1:7447`）。

按 `--hz` 的频率发布：

| 实体 | 类型 | 内容 |
|---|---|---|
| `world/lidar` | `points3d` | 模拟激光雷达点云（`xyz_f32` 二进制） |
| `world/vehicle` | `transform3d` | 车沿 8 字形路径的位姿 |
| `world/path` | `linestrips3d` | 车走过的轨迹 |
| `imu/accel_x`、`imu/accel_y`、`imu/accel_z`、`imu/gyro_z` | `scalar` | 模拟 IMU（在 Rerun 里是时间序列曲线） |

另外每秒一条 `log`，每 2 秒一轮 `node_definition` 和 `graph_update`（3 个假节点 `sim`、`planner`、`bridge`）。发布逻辑在库里的 `dorobot_plugin_dviz::fake::run_for(FakeOptions, Duration)`，示例只是命令行外壳；模拟器 `fake::sim` 搬自 dviz（见 `NOTICE`）。

## 端到端手动检查（需要有桌面的机器）

自动测试覆盖协议、路由、会话和发布器，但下面这些必须在有显示器的机器上人工确认。

1. 先装好 `rerun-cli 0.28`，再启动发布器（见上）。
2. 启动 Studio：`cargo run --locked -p dorobot-flex`（默认特性）。
3. 进入“3D visualization”，Mode `client`，Endpoint 填发布器地址，点“Connect”。状态变为 “Connected”。
4. 点“Open 3D window”：状态栏先显示 “3D window: starting…”，再变成 “3D window: open (Rerun 0.28.x)”。Rerun 窗口里能看到点云（`world/lidar`）、车的位姿（`world/vehicle`）和路径（`world/path`），`imu/*` 是随时间变化的曲线（时间序列视图）。
5. 面板里：Displays 有 4 个以上实体且计数增长；Logs 每秒新增一行；Nodes 里有 `sim`、`planner`、`bridge` 三个节点。
6. 切换“List / Graph”：两种视图都能切换。
7. 数据流图（来自界面评审，只有显示器能确认）：
   - 节点标签垂直居中、没有被截断；
   - 节点详情里的 ← / → 字形能正常显示；
   - 用真实鼠标点击图中的节点会选中它并显示详情。
8. 状态栏和日志文字在浅色和深色主题下都清晰可读。
9. 切到别的页面再回来，连接还在、数据继续更新。
10. 点“Disconnect”后发布器的数据不再增加；关掉 Rerun 窗口后可再点“Open 3D window”重开。

## 已知限制

- 3D 视图是独立的 Rerun 窗口，不嵌在 Studio 内。
- 本阶段（P1）只做通用 dviz 接入；接入 DORA_NAV 的真实话题留给 P2。
- Rerun 查看器版本必须是 0.28。
- “3D window: open” 只说明启动那一刻连上了；之后关掉 Rerun 窗口，这一行不会变，此后的 3D 数据直接丢弃，再点“Open 3D window”会重新启动。
- 9876 端口上如果已经有进程在监听，Studio 当它是 Rerun 查看器直接连过去（和 Rerun SDK 自己的做法一样），版本只核对 `PATH` 里的 `rerun`。
