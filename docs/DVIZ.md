# 3D visualization（dviz 插件）

插件 `dorobot.dviz`（crate `plugins/dviz`，`dorobot-plugin-dviz`）把机器人通过 Zenoh 发布的 dviz 消息显示出来：3D 内容（点云、位姿、路径、箭头、方框、文字、标量）送进独立的 Rerun 查看器，日志和数据流图显示在 Studio 的“Visualization”面板里。协议和 Rerun 写入逻辑来自 dviz 仓库（见仓库根的 `NOTICE`），本插件按 Apache-2.0 分发。

## 前提

- Rerun 0.28 查看器在 `PATH` 里：`cargo install rerun-cli --version 0.28.x --locked`（命令行名为 `rerun`）。没有它，“Open 3D window”会报错，其余功能照常。
- 构建带 Rerun：`dorobot-flex` 的默认特性已包含 `rerun`；`--no-default-features` 构建不含 Rerun，“Open 3D window”会提示“This build does not include Rerun”。
- 3D 内容在独立的 Rerun 窗口里，不嵌在 Studio 内。

## 连接

在侧栏进入“3D visualization”，面板“Visualization”里有四个输入：

| 输入 | 含义 |
|---|---|
| Mode | `peer`（默认）或 `client` |
| Endpoint | Zenoh 端点，如 `tcp/192.168.1.186:7447`；多个用逗号或空格分隔 |
| Prefix | 话题前缀，默认 `dviz`；订阅 `<prefix>/**`，Rerun 实体路径 = 话题去掉 `<prefix>/` |

- **同一局域网**：Mode 填 `peer`，Endpoint 留空，用组播自动发现。
- **跨 Wi-Fi / 组播不通**：Endpoint 填机器人上监听的地址；`client` 模式必须填 Endpoint，`peer` 模式填了就直连该地址。

点“Connect”建立会话，“Disconnect”断开，“Open 3D window”打开 Rerun。切到别的页面再回来，连接和已收到的数据保留（会话是进程内全局的）。状态栏显示连接状态和最近的错误，端点连不上会在 1 秒内给出带端点名的错误并自动重试。

面板内容：

- **Displays**：每个收到的实体一行，带开关和消息计数；关掉后该实体不再送进 Rerun。
- **Logs**：最近的日志，“WARN and above”只看警告及以上。
- **Nodes**：节点“List”或“Graph”（数据流图）。图里点击节点显示它的输入输出和上下游。

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
| `imu/accel_x`、`imu/accel_y`、`imu/accel_z`、`imu/gyro_z` | `scalar` | 模拟 IMU |

另外每秒一条 `log`，每 2 秒一轮 `node_definition` 和 `graph_update`（3 个假节点 `sim`、`planner`、`bridge`）。发布逻辑在库里的 `dorobot_plugin_dviz::fake::run_for(FakeOptions, Duration)`，示例只是命令行外壳；模拟器 `fake::sim` 搬自 dviz（见 `NOTICE`）。

## 端到端手动检查（需要有桌面的机器）

自动测试覆盖协议、路由、会话和发布器，但下面这些必须在有显示器的机器上人工确认。

1. 先装好 `rerun-cli 0.28`，再启动发布器（见上）。
2. 启动 Studio：`cargo run --locked -p dorobot-flex`（默认特性）。
3. 进入“3D visualization”，Mode `client`，Endpoint 填发布器地址，点“Connect”。状态变为已连接。
4. 点“Open 3D window”：Rerun 窗口里能看到点云（`world/lidar`）、车的位姿（`world/vehicle`）和路径（`world/path`），IMU 曲线在 `imu/*`。
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
