# dviz P2 交付③：RViz 外观、Rerun 改进和代价地图 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal：** Studio 的 Navigation 2D 地图和 Rerun 3D 窗口看起来像 RViz Nav2 的画面，并显示代价地图和 Tracer Mini 车体模型；顺带补上交付①留下的开机安全小问题。

**Architecture：**
- **DORA_NAV：** A* 规划器多一个 `costmap` 输出（Nav2 语义，0.05 m，≤ 1 Hz，有变化才发，每 5 s 补发）。
- **桥节点：** 把代价地图转成 dviz `grid2d`（costmap），并换上 RViz 式的默认颜色和线宽。
- **Studio 插件：** 用 RViz 的 map / costmap 调色板画 2D；Rerun 侧加网格、目标圆环、红色位姿箭头、代价地图和车体模型；关掉的实体从 Rerun 清除。

**Tech Stack：** 同交付①。Rust（Makepad `44d8220f`、Zenoh `=1.10.2`、Rerun 0.28、dora-node-api `=1.1.0`）；C++17（DORA_NAV，CMake + gtest）；bash / Python 3（autostart、离线精简脚本，只用 numpy）。

**Spec：**
- `docs/superpowers/specs/2026-11-23-dviz-p2-rviz-costmap-design.md`（下称"设计"）；
- 前身：`docs/superpowers/specs/2026-11-22-dviz-p2-nav-view-design.md`；
- 交付①计划：`docs/superpowers/plans/2026-11-23-dviz-p2-view.md`，其全局约束仍然有效。

## Global Constraints

- **交付①的全局约束全部有效：** Makepad 版本、`--locked`、Zenoh `=1.10.2`、Rerun 0.28 在 `rerun` 特性后面、桥节点独立工作区、不拖累导航、Studio 构建前缀 `LIBRARY_PATH=$HOME/nvme/dora-lab/linkshim` 和 `TMPDIR=$HOME/nvme/dora-lab/tmp`、`dorobot-flex` 只用 `--no-default-features`、不用 Rust 1.95 之后的特性、界面文案英文。
- **分支：**
  - Studio 继续在 `feat/dviz-p2`（当前 `d8443d4`）；
  - DORA_NAV 新分支 `feature/viz-costmap`，从 `feature/viz-bridge` 的 `2ebe6b5` 开。
- **提交身份：**
  - Studio：`git -c user.name=bobdingai -c user.email=dingxinmin@gmail.com commit`；
  - DORA_NAV：`git -c user.name=bobd988 -c user.email=dingxinmin.bob@gmail.com commit`；
  - 提交信息最后一行 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`；
  - 只在本地。
- **车上正在跑的东西不能动：**
  - 车上 `tracer-nav` 在跑：桥节点在 `/home/nvidia/nvme/dora-lab/opt/nav-bridge`，规划器是 DORA_NAV 的 `build/` 里那份。
  - **实现者一律不改 DORA_NAV 的 `build/`**：DORA_NAV 的构建和测试用单独的目录 `cmake -B $HOME/nvme/dora-lab/tmp/dora-nav-build`。桥节点的 release 版用 `--target-dir $HOME/nvme/dora-lab/tmp/nav-bridge-target`。
  - 不运行 `dora`、`autostart.sh` / `autostop.sh`（只能通过已有的 pytest，它会先把假的 `dora` 放在 PATH 最前面）、`systemctl`、`install.sh` 到 `opt/`、CAN 工具；
  - 不用 `pgrep -f` / `pkill -f` 匹配自己命令行里的字符串。
  - 部署和实车只由主控在用户同意后做（任务 8）。
- **代价地图：**
  - Nav2 语义：0 空闲，1–252 膨胀，253 内切，254 致命，255 未知；
  - 不降采样，0.05 m；第 0 行是 y 最小的一行；
  - 规划器发送：有变化时 ≤ 1 Hz，另每 5 s 补发；
  - 桥节点 `costmap/global` 频率上限 1 Hz，每 5 s 补发。
- **RViz 调色板：**
  - **map：** 占用值 i ∈ 0..=100 → 灰度 `v = 255 − 255·i/100`；未知 → `#708986`；
  - **costmap：** RViz 值 0 → 透明；1..=98 → `v = 255·i/100`，`(v, 0, 255−v)`；99 → `(0,255,255)`；100 → `(255,0,255)`；未知 → `#708986`；
  - **Nav2 代价换算成 RViz 值：** 0→0，253→99，254→100，255→未知，1..=252 → `1 + (c−1)·97/251`（整数除法；252→98）。
- **默认颜色和线宽**（桥节点配置，可改）：

  | 元素 | RGBA | 2D 线宽 | Rerun 半径 |
  |---|---|---|---|
  | 规划 | `[0,180,255,255]` | 4 px | 0.025 m |
  | 实际轨迹 | `[0,220,0,255]` | 3 px | 0.02 m |
  | 目标 | `[255,170,0,255]` | 圆环 2 px，半径 0.15 m（最小 10 px），有朝向时加 0.5 m 箭头 | 0.015 m |
  | 足迹 | `[220,220,220,255]` | 2 px，内部填 `[40,40,40,120]` | 0.01 m |
  | 位姿箭头 | `[255,25,0,255]` | 长 0.4 m，2 px | 0.015 m |
  | 不可信时的足迹 | `[255,0,0,255]` | — | — |
  | 激光 | `[255,0,0,255]` | 3 px 方点 | — |
  | PCD 地图 | `[180,180,180,255]` | — | — |

  "不可信"的定义：`loc_ok == false`，或 `stale` 里有 `"localization"`。
- **2D 画布：**
  - 背景 `#303030`，两种主题都一样；
  - 网格 1 m，`[160,160,164,128]`；
  - 地图不透明度 0.7（alpha 179）；代价地图不透明度 0.5（alpha 128）。
- **Rerun：**
  - 实时点云半径 0.01 m，alpha 153（0.6），按高度着色；
  - PCD 地图 `map/cloud` 默认不发（Displays 里默认不勾）；
  - 关掉的实体发 `Clear`；
  - 代价地图画在 z = 0.01 m；
  - 网格 1 m，z = 0.002 m。
- **车体模型：**
  - 来源 `~/nvme/tracer_ws/src/tracer-mini-ros2/tracer_mini_description/meshes/base_link.STL`（Apache-2.0，见其 `package.xml`）；
  - 精简后 ≤ 1 MB；
  - 对齐：水平中心对准 (x = −0.11, y = 0)，底面 z = 0，网格 x 朝车头；
  - `robot_model` 为 `"tracer_mini"`；
  - 颜色深灰 `[64,64,64,255]`；
  - 在 NOTICE 里写明来源和许可证。
- **带宽：** 桥节点总发送量 ≤ 1 MB/s。

## Review Focus

1. **障碍层随激光不停变化：** 代价地图仍然 ≤ 1 Hz，总带宽 ≤ 1 MB/s。测试见任务 2 `costmap_throttle_caps_rate_and_resends`、任务 3 `costmap_forward_is_rate_limited`，实车用任务 8 的探测程序核对。
2. **Studio 晚连上：** 5 s 内看到代价地图。测试见任务 3 `late_subscriber_gets_costmap_within_six_seconds`。
3. **车体模型资源缺失、损坏，或 `robot_model` 是不认识的名字：** 不画模型，不崩溃，2D 照常画足迹。测试见任务 7 `unknown_robot_model_logs_nothing` 和 `corrupt_model_asset_is_an_error_not_a_panic`。
4. **Displays 里反复开关某个实体：** 每次关都发一次 `Clear`，再开就恢复显示，界面线程从不直接调用写 3D 数据的组件。测试见任务 6 `disable_sends_clear_from_the_worker_once`。
5. **没有代价地图**（规划器没升级，或图例里关掉）：2D 和 Rerun 照常画其余各层。测试见任务 4 `scene_without_costmap_draws_nothing_for_it` 和任务 5 `costmap_layer_off_skips_it`。

---

## 文件结构

**DORA_NAV（`feature/viz-costmap`）**

| 文件 | 职责 | 任务 |
|---|---|---|
| `apps/tracer/autostart.sh`、`apps/tracer/test_viz_overlay.py` | 开机小问题 A1、A2 | 1 |
| `modules/planner/nav_costmap/src/costmap2d.h/.cpp` | 补 `originX()`、`originY()`、`costs()` | 2 |
| `modules/planner/nav_costmap/src/costmap_wire.h/.cpp`（新） | 代价地图线上格式 | 2 |
| `modules/planner/global_planner/astar_planner_dora/src/costmap_throttle.h/.cpp`（新） | 发送节奏 | 2 |
| `modules/planner/global_planner/astar_planner_dora/src/astar_planner_dora_node.cpp` | 发 `costmap` | 2 |
| `apps/tracer/tracer_nav.yml`、`apps/tracer/viz_bridge.yml.in` | 声明输出、接第 11 个输入 | 2 |
| `msg/README.md` | 格式说明 | 2 |

**Studio（`feat/dviz-p2`）**

| 文件 | 职责 | 任务 |
|---|---|---|
| `crates/dviz-protocol/src/lib.rs` | `COSTMAP_GLOBAL`、`NavStatus.robot_model`、线和箭头的 `radius` | 3 |
| `runtime/nav-bridge/src/{config,formats,worker,encode,status}.rs`、`config/tracer.json`、`install.sh`、`README.md`、`tests/replay/*` | 代价地图转发、新颜色和线宽、`robot_model`、A3 | 3 |
| `plugins/dviz/src/map_view.rs`、`nav_scene.rs` | 调色板、换算、网格线、代价地图层 | 4 |
| `plugins/dviz/src/map2d.rs`、`nav_panel.rs` | 2D 绘制、图例 | 5 |
| `plugins/dviz/src/rerun_sink.rs`、`route.rs`、`session.rs` | Rerun 各项改进、`Clear`、默认不发 | 6 |
| `plugins/dviz/assets/tracer_mini.stl`、`plugins/dviz/assets/NOTICE-tracer-mini.md`、`plugins/dviz/tools/make_tracer_model.py`（新）、`rerun_sink.rs` | 车体模型 | 7 |
| `docs/DVIZ.md` | 文档 | 5、6、7 |

---

### Task 1: 开机小问题 A1、A2（DORA_NAV `autostart.sh`）

**Files:**
- Modify: `apps/tracer/autostart.sh`（`wait_s=${NAV_VIZ_START_WAIT:-20}` 一行，以及 `elif ! "${DORA[@]}" start tracer_nav.viz.yml` 分支）
- Test: `apps/tracer/test_viz_overlay.py`

**Interfaces：** 不变。
- 先建分支：`git -C /home/nvidia/nvme/DORA_NAV checkout -b feature/viz-costmap 2ebe6b5`

- [ ] **Step 1：写失败的测试**（照现有的 `run_autostart` 和假 `dora` 写法）：
  - `test_non_integer_start_wait_uses_default`：
    - `NAV_VIZ_START_WAIT=30s`，假 `dora` 第一次 `list` 就报带桥节点的数据流 Running、节点数正确、桥节点日志有启动行；
    - 断言退出码 0、打印 "not a whole number"（文案由实现者定，要包含变量名），并且调用序列是 `[viz start, list]`。
  - `test_viz_start_failing_stops_before_plain`：
    - 假 `dora` 让 `start tracer_nav.viz.yml` 返回 1，之后 `list` 先报一次 tracer-nav Running、再报没有；
    - 断言调用序列是 `[viz start, stop --name tracer-nav --grace-duration 10s, list, list, plain start]`。
- [ ] **Step 2：确认失败：** `cd apps/tracer && python3 -m pytest -q test_viz_overlay.py -k "non_integer or failing_stops"`
- [ ] **Step 3：实现：**
  - `wait_s` 不匹配 `^[0-9]+$` 时，打印原因到 stderr，取 20；
  - `dora start` 失败的分支，复用"停掉 + 最多等 15 s 到没有 Running 的 tracer-nav"那段逻辑（抽成函数 `stop_viz_dataflow`），再起原数据流。
- [ ] **Step 4：确认通过：** 全部 pytest 通过，`bash -n autostart.sh`。
- [ ] **Step 5：提交** `fix(apps/tracer): validate NAV_VIZ_START_WAIT; stop before the plain fallback`

### Task 2: 规划器输出代价地图（DORA_NAV）

**Files:**
- Modify: `modules/planner/nav_costmap/src/costmap2d.h/.cpp`（加只读访问）
- Create: `modules/planner/nav_costmap/src/costmap_wire.h/.cpp`
- Modify: `modules/planner/nav_costmap/CMakeLists.txt`（编进 `nav_costmap` 库）
- Create: `modules/planner/global_planner/astar_planner_dora/src/costmap_throttle.h/.cpp`（编进 `astar_planner_lib`）
- Modify: `modules/planner/global_planner/astar_planner_dora/src/astar_planner_dora_node.cpp`（每个事件处理完后检查是否要发 `costmap`）
- Modify: `apps/tracer/tracer_nav.yml`（`astar_planner` 的 `outputs: [path, costmap]`）
- Modify: `apps/tracer/viz_bridge.yml.in`（加 `costmap: {source: astar_planner/costmap, queue_size: 2}`）
- Modify: `msg/README.md`（写清格式）
- Test: `modules/planner/nav_costmap/test/test_costmap_wire.cpp`（新）、`modules/planner/global_planner/astar_planner_dora/test/test_planner.cpp`、`apps/tracer/test_viz_overlay.py`

**Interfaces（Produces）：**

```cpp
// costmap2d.h
double originX() const; double originY() const;
const std::vector<uint8_t>& costs() const;   // cost_，行优先，第 0 行 y 最小

// costmap_wire.h（namespace nav_costmap）
// 小端：f64 stamp, u32 width, u32 height, f32 resolution, f32 origin_x, f32 origin_y, f32 origin_yaw(=0)
// = 32 字节头，然后 width*height 个 u8（Nav2 语义），总长 32 + w*h
constexpr size_t kCostmapHeader = 32;
std::string serializeCostmap(const Costmap2D& c, double stamp);
struct CostmapMsg { double stamp; uint32_t w, h; float res, ox, oy, oyaw; std::vector<uint8_t> cells; };
bool parseCostmap(const uint8_t* p, size_t n, CostmapMsg& out);
uint64_t hashCosts(const std::vector<uint8_t>& costs);   // FNV-1a 64

// costmap_throttle.h（namespace astar_planner）
class CostmapThrottle {
 public:
  CostmapThrottle(double min_period_s = 1.0, double resend_s = 5.0);
  bool due(double t, uint64_t hash);   // 距上次 ≥ min_period 且（hash 变了 或 距上次 ≥ resend）
  void sent(double t, uint64_t hash);
};
```

- [ ] **Step 1：配置单独的构建目录**（不碰 `build/`）：
  `cmake -S /home/nvidia/nvme/DORA_NAV -B $HOME/nvme/dora-lab/tmp/dora-nav-build <与 build/ 相同的选项>`。选项从 `build/CMakeCache.txt` 里读出来（`CMAKE_BUILD_TYPE`，以及依赖路径，例如 `CMAKE_PREFIX_PATH`）。
- [ ] **Step 2：写失败的测试：**
  - `CostmapWire.RoundTrip`：用 4×3 的 `OccupancyGrid` 建 `Costmap2D`，序列化再解析，头字段相等，`cells` 和 `costs()` 相等，长度 = 32 + 12。
  - `CostmapWire.RejectsBadLength`：截掉 1 字节、头里宽高与长度不符，都返回 false。
  - `CostmapWire.RowZeroIsMinY`：在 y 最小的那一行放一个致命格，序列化后它在第 0 行。
  - `costmap_throttle_caps_rate_and_resends`（`test_planner.cpp`）：
    - 0.2 s 一次、hash 每次都变，持续 10 s：`due` 为真的次数 ≤ 11；
    - hash 不变时，只在 ≥ 5 s 的间隔为真；
    - 第一次调用为真。
  - pytest `test_overlay_sources_are_outputs_of_the_real_dataflow` 已经存在，在模板加第 11 个输入后必须仍然通过（证明 `tracer_nav.yml` 已声明 `costmap`）。另加 `test_template_has_costmap_input`。
- [ ] **Step 3：确认失败：**
  - `cmake --build $HOME/nvme/dora-lab/tmp/dora-nav-build -j8 && ctest --test-dir $HOME/nvme/dora-lab/tmp/dora-nav-build -R "CostmapWire|costmap_throttle"`
  - 再跑 pytest。
- [ ] **Step 4：实现。**
  - 节点里每次处理完事件、发完 path 之后：`hash = hashCosts(planner.costmap().costs())`；`due` 为真就 `serializeCostmap` 并 `dora_send_output("costmap")`，然后 `sent`。
  - 用 `std::chrono` 测这段的耗时，每 100 次打印一次最大值（stdout）。
- [ ] **Step 5：确认通过：**
  - 单独构建目录里的全部 ctest：原来 307 条加上新增的，0 失败；
  - pytest 全部通过；
  - 在测试里或用一个小基准打印 187×208 编码加哈希的耗时，要求 < 2 ms（Orin）。
- [ ] **Step 6：** 在 `msg/README.md` 写代价地图格式。提交 `feat(planner): costmap output (Nav2 costs, 0.05 m, ≤ 1 Hz, resend 5 s)`。

### Task 3: 协议和桥节点（代价地图转发、新颜色和线宽、`robot_model`、A3）

**Files:**
- Modify: `crates/dviz-protocol/src/lib.rs`
- Modify: `runtime/nav-bridge/src/{config.rs,formats.rs,worker.rs,encode.rs,status.rs}`、`config/tracer.json`、`install.sh`、`README.md`
- Modify: `runtime/nav-bridge/tests/replay/{nav_replay.yml.in,expect.json}`、`examples/dviz_probe.rs`（如果需要）
- Test: 各文件的 `mod tests`、`tests/zenoh_loop.rs`、`tests/install_sh.rs`

**Interfaces（Produces）：**

```rust
// dviz-protocol
pub mod nav_entities { pub const COSTMAP_GLOBAL: &str = "costmap/global"; pub const ALL: [&str; 11] = [/* 原 10 个 + COSTMAP_GLOBAL */]; }
pub struct NavStatus { /* …原字段… */ #[serde(default)] pub robot_model: Option<String> }
pub struct LineStrips3DData { /* … */ #[serde(default)] pub radius: Option<f32> }
pub struct Arrows3DData { /* … */ #[serde(default)] pub radius: Option<f32> }

// nav-bridge
pub enum InputFormat { /* … */ CostmapU8 }      // 配置名 "costmap"
pub fn parse_costmap(b: &[u8]) -> Result<(Grid2DHeader, Vec<u8>), String>;  // 32 字节头布局同任务 2；kind = Costmap；origin = [ox, oy, oyaw]
pub fn enc_strip(entity: &'static str, pts: &[[f32; 2]], color: [u8; 4], radius: f32) -> (&'static str, Vec<u8>);
pub fn enc_goal(g: Option<&GoalMsg>, color: [u8; 4], radius: f32) -> (&'static str, Vec<u8>);
// BridgeConfig 新字段：robot_model: Option<String>（默认 Some("tracer_mini")）；Colors 改成全局约束里的默认值；
// Radii { plan: 0.025, trail: 0.02, goal: 0.015, footprint: 0.01 }；rates_hz.costmap = 1.0
```

- [ ] **Step 1：写失败的测试：**
  - **协议：**
    - `nav_entities_all_distinct` 改为 11 个；
    - `nav_status_robot_model_defaults_none`：旧 JSON 缺字段时为 None；
    - `strip_radius_round_trip`。
  - **桥节点：**
    - `parse_costmap_layout`：手工拼 32 字节头加 6 个格；长度错就返回 Err；
    - `costmap_forward_is_rate_limited`：1 s 内推 10 条 costmap，发出 ≤ 2 条；
    - `late_subscriber_gets_costmap_within_six_seconds`（zenoh_loop，照交付①的晚连测试写法）；
    - `default_colors_are_rviz`：`BridgeConfig::default().colors` 和 `config/tracer.json` 都等于全局约束表里的值；
    - `status_carries_robot_model`：`build()` 出来的 `NavStatus.robot_model == Some("tracer_mini")`；
    - `strips_carry_radius`：编码后 JSON 里有 `radius`。
  - **A3：**
    - `install.sh` 拒绝含 `#`、`$`、`;`、非 ASCII 字符的路径，和 `viz_overlay.py` 的 `PATH_OK` 一致：`[A-Za-z0-9_./+,:=%~-]+`；
    - README 里不再出现 `target/release/dorobot-nav-bridge` 作为数据流示例。
- [ ] **Step 2：确认失败：** `cargo test --locked -p dviz-protocol`；`cd runtime/nav-bridge && cargo test --locked`
- [ ] **Step 3：实现。**
  - 配置的输入映射加 `"costmap": "CostmapU8"`；
  - 回放数据流的模板也接上 `costmap`，`expect.json` 加 `costmap/global`（≤ 1.1 Hz，必须出现）；
  - 探测程序里实体从 `ALL` 来，自动包含新的那个。
- [ ] **Step 4：确认通过：**
  - 桥节点 `cargo test --locked`（Zenoh 测试 3 遍）、clippy；
  - 插件两种构建的测试（协议的 `ALL` 变了，插件有引用的地方要跟着改）；
  - `cargo test --locked -p dorobot-flex --no-default-features --test plugins`。
- [ ] **Step 5：提交** `feat(nav-bridge): forward the costmap; RViz colours and widths; robot_model`

### Task 4: 调色板、换算和代价地图层（插件纯计算）

**Files:**
- Modify: `plugins/dviz/src/map_view.rs`、`plugins/dviz/src/nav_scene.rs`
- Test: 两个文件的 `mod tests`

**Interfaces（Produces）：**

```rust
// map_view.rs
pub fn nav2_to_rviz(cost: u8) -> Option<u8>;              // None = 未知；0→0，253→99，254→100，1..=252→1+(c-1)*97/251
pub fn rviz_map_color(occ: u8) -> [u8; 4];                // occ 是 grid2d occupancy 值（0..=100，255 未知）；alpha 179
pub fn rviz_costmap_color(v: Option<u8>) -> [u8; 4];      // alpha：0 → 0，其余 128；未知 #708986 alpha 128
pub fn grid_rgba(header: &Grid2DHeader, cells: &[u8]) -> Vec<u8>;    // 改为 RViz map 调色板（行序不变：第 0 行是 y 最大）
pub fn costmap_rgba(header: &Grid2DHeader, cells: &[u8]) -> Vec<u8>; // RViz costmap 调色板，同样行序
pub fn grid_lines(view: &MapView, spacing_m: f64) -> Vec<([f64; 2], [f64; 2])>; // 屏幕坐标线段，覆盖可见区域
pub struct NavLayers { /* … */ pub costmap: bool }        // 默认 true

// nav_scene.rs
pub enum NavLayer { /* … */ Costmap }                     // nav_layer("…/costmap/global") → Costmap
pub struct NavScene { /* … */ pub costmap: Option<GridLayer> }
```

- [ ] **Step 1：写失败的测试：**
  - `nav2_to_rviz_table`：0→Some(0)，1→Some(1)，252→Some(98)，253→Some(99)，254→Some(100)，255→None。
  - `rviz_map_palette`：0→`[255,255,255,179]`，100→`[0,0,0,179]`，50→`[128,128,128,179]`（`255 − 127`），255→`[0x70,0x89,0x86,179]`。
  - `rviz_costmap_palette`：0→alpha 0，1→`[2,0,253,128]`，98→`[249,0,6,128]`，99→`[0,255,255,128]`，100→`[255,0,255,128]`，None→`[0x70,0x89,0x86,128]`。
  - `grid_lines_follow_zoom`：`MapView{center:[0,0], px_per_m:100, size:[800,600]}` 时，竖线 9 条（x = −4..=4 m），第一条在屏幕 x = 0；放大一倍后竖线变成 5 条。
  - `costmap_layer_parsed`：一条 `grid2d`（kind costmap，实体 `tracer/costmap/global`），`scene.costmap` 有值，`generation` 是 1。
  - `scene_without_costmap_draws_nothing_for_it`：`costmap_rgba` 不会在没有代价地图时被调用。用 `scene.costmap.is_none()` 加 `scene_bounds` 不变来断言。
  - 原有的 `grid_rgba_colors_and_flip` 按新调色板更新期望值。
- [ ] **Step 2：确认失败。**
- [ ] **Step 3：实现。**
- [ ] **Step 4：确认通过**（两种构建）。
- [ ] **Step 5：提交** `feat(plugins/dviz): RViz map and costmap palettes, grid lines, costmap layer`

### Task 5: 2D 画面（Navigation 面板）

**Files:**
- Modify: `plugins/dviz/src/map2d.rs`、`plugins/dviz/src/nav_panel.rs`、`docs/DVIZ.md`（"连接车上的 DORA_NAV"那一节的颜色表）
- Test: `plugins/dviz/src/map2d.rs` 的纯函数测试、`plugins/dviz/tests/panel.rs`、`dorobot-flex/tests/plugins.rs`

**Interfaces：**
- **Consumes：** 任务 4 的全部接口。
- **Produces（纯函数，供测试）：**

  ```rust
  pub fn goal_ring(center: [f64; 2], px_per_m: f64) -> Vec<[f64; 2]>;  // 屏幕点，24 段闭合；半径 max(0.15 m, 10 px)
  pub fn pose_arrow(pose: &Pose2D) -> ([f64; 2], [f64; 2]);            // 世界坐标：车位置 → 车头方向 0.4 m
  pub fn footprint_color(status: Option<&NavStatus>, normal: [u8; 4]) -> [u8; 4]; // 不可信或过期 → [255,0,0,255]
  ```

- **绘制顺序**（从下到上）：
  1. 画布 `#303030`
  2. 地图贴图（alpha 0.7）
  3. 代价地图贴图（alpha 0.5，只在 `layers.costmap` 时画）
  4. 网格线
  5. 规划 4 px
  6. 实际轨迹 3 px
  7. 激光 3 px 方点
  8. 足迹：先填充，再画 2 px 边
  9. 红色位姿箭头
  10. 目标：圆环，有朝向时加箭头（取代交付①的十字）
- 线宽和颜色以全局约束为准；桥节点发来的颜色优先。
- 图例加 "Costmap" 一项，用紫色块 `[255,0,255,255]` 表示，默认勾选；色块跟随新颜色。

- [ ] **Step 1：写失败的测试：**
  - `goal_ring_radius_min_10px`：px_per_m 20 时半径 10 px，px_per_m 200 时半径 30 px，点数 25（首尾相同）。
  - `pose_arrow_points_forward`：yaw π/2 时终点是 (x, y + 0.4)。
  - `footprint_red_when_untrusted_or_stale`：`loc_ok == false` 为红；`stale` 含 `"localization"` 为红；正常时用 normal。
  - `costmap_layer_off_skips_it`：图例里取消 Costmap 后，绘制计划（实现者给出可测的"本帧要画哪些层"的纯函数）里没有代价地图。
  - `nav_panel_legend_has_costmap`（panel 测试）。
- [ ] **Step 2：确认失败。**
- [ ] **Step 3：实现。** `DvizMap2D` 的第二张贴图（代价地图）照地图贴图的缓存规则：只在内容真正变化时重建。
- [ ] **Step 4：确认通过：** 两种构建的插件测试、`dorobot-flex` 的 plugins 测试、一次 `--workspace --no-default-features`。
- [ ] **Step 5：提交** `feat(plugins/dviz): RViz-style 2D map (dark canvas, grid, palettes, ring, pose arrow)`

### Task 6: Rerun 改进（线宽、点云、代价地图、网格、圆环、位姿箭头、清除、PCD 默认不发）

**Files:**
- Modify: `plugins/dviz/src/rerun_sink.rs`、`plugins/dviz/src/route.rs`、`plugins/dviz/src/session.rs`、`docs/DVIZ.md`
- Test: `rerun_sink.rs` 的 `mod tests`、`plugins/dviz/tests/loopback.rs`

**Interfaces（Produces）：**
- `Sink` trait 加 `fn clear(&mut self, entity: &str) -> Result<(), String> { Ok(()) }`。`RerunSink` 的实现是记 `rerun::Clear::recursive()`。
- `DvizSession::set_entity_enabled(entity, false)` 只是把实体放进待清除队列；**工作线程**在处理下一条消息前（或空闲时最迟 100 ms 内）调用 `sink.clear`。界面线程从不调用 sink。
- 实体登记时，`nav_layer == MapCloud` 的默认 `enabled = false`。
- **RerunSink 的行为：**
  - `linestrips3d` / `arrows3d` 带 `radius` 时用 `with_radii`；
  - `NavLayer::Cloud` 的点用半径 0.01 m、alpha 153；
  - `grid2d` 的 kind 为 costmap 时：只画非空闲格（RViz 值 > 0 或未知），每格一个四边形，整体做成一个 `Mesh3D`，用顶点颜色，z = 0.01 m，静态记录；
  - **为什么不用贴图：** Rerun 0.28 显示贴图时忽略透明度。这是对设计"画成贴图"的修改，写进报告。
  - **网格：** 收到第一张 `map/grid` 后，在 `<robot>/view/grid` 记静态的 1 m 网格线，覆盖地图范围，颜色 `[160,160,164,128]`，半径 0.003，z = 0.002。如果 Rerun 0.28 的 Rust SDK 能方便地发 `LineGrid3D` blueprint，也可以用它代替。二选一，写进报告。
  - **目标圆环：** 收到 `nav/goal` 时，另在 `<goal 实体>/ring` 画半径 0.15 m 的圆环（24 段，橙黄，半径 0.015）；没有目标时清掉。
  - **位姿箭头：** `transform3d` 实体下另记 `<pose 实体>/heading`，红色 `Arrows3D`，从原点指向 (0.4, 0, 0)，随位姿移动。

- [ ] **Step 1：写失败的测试：**
  - `strips_use_radius`：带 `radius: 0.025` 的 strip 写出 `LineStrips3D:radii`，值为 0.025。
  - `cloud_points_small_and_translucent`：`tracer/sensors/cloud` 写出的半径是 0.01，颜色 alpha 是 153。
  - `costmap_is_a_vertex_colored_mesh`：3×1 代价地图 `[0,254,253]`，写出的 `Mesh3D` 有 2 个四边形（8 个顶点，或按实现的顶点数），顶点颜色里有紫色和青色，z 都是 0.01。
  - `grid_lines_logged_once_per_map`：两次 `map/grid` 只写一次网格（或按 blueprint 的等价检查）。
  - `goal_ring_and_pose_heading`：分别写出 `…/ring` 和 `…/heading`。
  - `disable_sends_clear_from_the_worker_once`（loopback）：
    - 用记录型 sink（记下 `clear` 调用时所在的线程）；
    - 从测试线程调用 `set_entity_enabled(e, false)`；
    - 1 s 内收到恰好一次 `clear(e)`，并且发生在 worker 线程；
    - 再启用后，下一条消息照常送达。
  - `map_cloud_disabled_by_default`：`tracer/map/cloud` 登记后 `enabled == false`，消息不进 sink。
- [ ] **Step 2：确认失败**（`--features rerun`，以及 loopback）。
- [ ] **Step 3：实现。**
- [ ] **Step 4：确认通过：** 两种构建；loopback 跑 3 遍。
- [ ] **Step 5：** 在 `docs/DVIZ.md` 写清：PCD 地图默认关、关掉即清除、Rerun 的网格、代价地图。提交 `feat(plugins/dviz): Rerun costmap, grid, ring, heading, radii; clear on disable`

### Task 7: Tracer Mini 车体模型

**Files:**
- Create: `plugins/dviz/tools/make_tracer_model.py`（一次性精简脚本，只用 numpy）
- Create: `plugins/dviz/assets/tracer_mini.stl`（脚本生成，≤ 1 MB，二进制 STL）
- Create: `plugins/dviz/assets/NOTICE-tracer-mini.md`（来源、许可证 Apache-2.0、修改说明）
- Modify: `plugins/dviz/src/rerun_sink.rs`（模型记录），以及 session / route 里触发模型记录的地方
- Modify: `NOTICE`（仓库根目录，加一行指向 `NOTICE-tracer-mini.md`）、`docs/DVIZ.md`
- Test: `rerun_sink.rs` 的 `mod tests`、`plugins/dviz/tests/loopback.rs`

**Interfaces（Produces）：**
- `make_tracer_model.py <src.stl> <dst.stl> --max-bytes 1000000 --center-x -0.11 --center-y 0 --floor-z 0`：
  - 读二进制 STL；
  - 用顶点聚类降采样，自动选格子大小，直到 ≤ max-bytes；
  - 平移，使包围盒水平中心到 (center-x, center-y)、最低点到 floor-z；
  - 网格 x 轴不旋转；
  - 打印三角形数、文件大小、包围盒。
- 模型编译时嵌进插件：`include_bytes!("../assets/tracer_mini.stl")`，只在 `rerun` 特性下。
- **触发：** `NavScene` 第一次看到 `status.robot_model == Some("tracer_mini")`（每次连接一次）时，session 向 sink 发一条合成的可视消息：
  - 实体：`<robot>/robot/pose/model`；
  - 消息：`MvizMessage{type: "model_ref", data: {"name": "tracer_mini"}}`。
- **RerunSink 处理 `model_ref`：**
  - 认识的名字：静态记录 `rerun::Asset3D::from_file_contents(bytes, Some(MediaType::stl()))`，颜色深灰 `[64,64,64,255]`（用 albedo factor 或同类设置）；
  - 不认识的名字：返回 Ok，什么都不写，记一次日志；
  - 资源解析失败：返回 Err，不 panic。

- [ ] **Step 1：运行脚本生成资源。**
  - `python3 plugins/dviz/tools/make_tracer_model.py ~/nvme/tracer_ws/src/tracer-mini-ros2/tracer_mini_description/meshes/base_link.STL plugins/dviz/assets/tracer_mini.stl`
  - 记录打印出的三角形数、大小和包围盒。大小要 ≤ 1 MB；包围盒的水平中心约 (−0.11, 0)，最低点 0。
- [ ] **Step 2：写失败的测试：**
  - `model_asset_small_and_aligned`：资源 ≤ 1 000 000 字节；解析出的包围盒中心 x 在 −0.11 ± 0.01，y 在 0 ± 0.01，最小 z 是 0 ± 0.001。
  - `model_ref_logs_static_asset`：sink 收到 `model_ref tracer_mini` 后，记录里有 `Asset3D` 组件，实体是 `…/robot/pose/model`，是静态的。
  - `unknown_robot_model_logs_nothing`：`model_ref foo` 返回 Ok，记录里没有 `Asset3D`。
  - `corrupt_model_asset_is_an_error_not_a_panic`：用一段截断的 STL 字节调用内部的加载函数，返回 Err。
  - `model_ref_sent_once_per_connection`（loopback）：连续多条带 `robot_model` 的 status 只触发一次；断开重连后再触发一次。
- [ ] **Step 3：确认失败。**
- [ ] **Step 4：实现**，写 NOTICE。
- [ ] **Step 5：确认通过**（两种构建、loopback 3 遍、一次 `--workspace --no-default-features`）。
- [ ] **Step 6：提交** `feat(plugins/dviz): Tracer Mini body model in Rerun`

### Task 8: 部署、实车核对和 epyc 截图（主控执行，每一步按用户的同意）

**Files：**
- Modify：`docs/superpowers/plans/2026-11-23-dviz-p2-view-progress.md`（交付③一节）

- [ ] **Step 1：先问用户**能否部署：重建 DORA_NAV 的 `build/` 里的规划器、重装桥节点、重启 dora-nav。底盘会重新进入 CAN 模式，不武装。同意后：
  1. 把 DORA_NAV 检出到 `feature/viz-costmap`；
  2. `cmake --build build --target astar_planner_dora_node`；
  3. 用 `install.sh` 装新版桥节点到 `/home/nvidia/nvme/dora-lab/opt/nav-bridge`；
  4. `systemctl --user restart dora-nav`；
  5. 核对：启动输出 "Running with the viz bridge (15 nodes)"，桥节点日志有 "started by Dora"。
- [ ] **Step 2：用探测程序测实车 60 s**（只读）：`dviz_probe --connect tcp/127.0.0.1:7450 --seconds 60 --expect tests/replay/expect.json`，要求 `costmap/global` ≤ 1.1 Hz、总带宽 ≤ 1 MB/s。结果写进进度文档。
- [ ] **Step 3：epyc 截图核对。** 同步分支并编译，重启 Studio（不操作用户的鼠标和键盘，界面操作请用户做，或在用户离开时由主控截图）。逐项核对：
  - **2D：** 深灰画布、网格、地图颜色、代价地图的紫 / 青 / 渐变带、浅蓝规划、绿色轨迹、橙黄目标圆环、红色位姿箭头、定位不可信时车框变红；
  - **Rerun：** 车体模型的朝向和位置、网格、点云变淡、PCD 地图默认不显示、关掉实体后被清除。

  模型朝向不对时，按设计退回"方块车体 + 圆柱轮子"，另开一个小任务。
- [ ] **Step 4：（可选）** 用户说 go 后，发一个短目标，看规划、实际轨迹和代价地图。
- [ ] **Step 5：提交**进度文档。
