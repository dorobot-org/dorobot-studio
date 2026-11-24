# 3D visualization（dviz 插件）

插件 `dorobot.dviz`（crate `plugins/dviz`，`dorobot-plugin-dviz`）把机器人通过 Zenoh 发布的 dviz 消息显示出来：3D 内容（点云、位姿、路径、箭头、方框、文字、标量）送进独立的 Rerun 查看器，日志和数据流图显示在 Studio 的“Visualization”面板里。协议和 Rerun 写入逻辑来自 dviz 仓库（见仓库根的 `NOTICE`），本插件按 Apache-2.0 分发。

## 前提

- Rerun 0.28 查看器在 `PATH` 里（命令行名为 `rerun`）：`python3 -m venv ~/rerun-0.28 && ~/rerun-0.28/bin/pip install rerun-sdk==0.28.2`，再把 `~/rerun-0.28/bin` 放进 `PATH`。`cargo install rerun-cli --version 0.28.2` 在 Rust 1.95 上编译失败（E0275），不要用。没有它，或者 `rerun --version` 的主、次版本号不是 0.28，“Open 3D window”都会在状态栏报错（版本不对时写出查看器和 Studio 内置 SDK 两边的版本），不会启动它；其余功能照常。
- 构建带 Rerun：`dorobot-flex` 的默认特性已包含 `rerun`；`--no-default-features` 构建不含 Rerun，“Open 3D window”会提示“This build does not include Rerun”。
- 3D 内容在独立的 Rerun 窗口里，不嵌在 Studio 内。

## 连接

在侧栏进入“3D visualization”，面板“Visualization”里有三个输入：

| 输入 | 含义 |
|---|---|
| Mode | `peer`（默认）或 `client` |
| Endpoint | Zenoh 端点，如 `tcp/192.168.1.160:7450`（车上的桥节点）；多个用逗号或空格分隔 |
| Prefix | 话题前缀，默认 `dviz`；订阅 `<prefix>/**`，Rerun 实体路径 = 话题去掉 `<prefix>/` |

- **同一局域网**：Mode 填 `peer`，Endpoint 留空，用组播自动发现。
- 组播模式下状态栏的 “Connected” 可能是局域网里别的 Zenoh 节点（比如车上的 Dora 节点），不一定是发布器；连车请用 `client` 模式并填端点。
- **跨 Wi-Fi / 组播不通**：Endpoint 填机器人上监听的地址；`client` 模式必须填 Endpoint，`peer` 模式填了就直连该地址。

点“Connect”建立会话，“Disconnect”断开，“Open 3D window”打开 Rerun。切到别的页面再回来，连接和已收到的数据保留（会话是进程内全局的）。这些按钮都立即返回，界面不会等 Zenoh 或 Rerun。

Mode、Endpoint 或 Prefix 和上次 Connect 不同（比如从仿真车换到真车）时，Connect 从头开始：实体、日志、节点、计数和最近一条命令的回执都清空，Rerun 里上一个连接画的东西也清掉（每个根实体，如 `tracer_sim`，记一条 `Clear`，在新连接的第一条数据之前）。设置不变再点 Connect 只是重连，什么都不清。

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

- **Displays**：每个收到的实体一行，带开关和消息计数。关掉后该实体不再送进 Rerun，Rerun 里已经画出的内容（连同它下面的子实体，如目标圆环、位姿箭头）也清掉：Studio 往 Rerun 记一条 `Clear`，静态记录的地图、代价地图另记一条静态的 `Clear`。再勾上，下一条消息到了就重新显示（地图和代价地图最多等 5 s 的补发；`robot/pose` 下的车体模型马上重新记一次）。清除由后台接收线程在下一条消息之前做（空闲时 50 ms 内），界面不等它；断开连接时关掉的实体要等下次连上才清。PCD 地图 `map/cloud` 默认不勾：它很大，勾上才送进 Rerun。
- **Logs**：最近的 12 条日志，最新的在最上面；每条只占一行，超过 160 个字符的截断并以“…”结尾。“WARN and above”只看警告及以上。
- **Nodes**：节点“List”或“Graph”（数据流图）。每个节点显示状态和 “seen at t=12.3 s”：t 是发送方的时钟（数据流启动以来的秒数），不是“多少秒以前”。图里点击节点显示它的输入输出和上下游。

## 连接车上的 DORA_NAV（Navigation 面板）

车上的桥节点 `dorobot-nav-bridge`（`runtime/nav-bridge`）挂在 DORA_NAV 的数据流里，把导航数据整理成 dviz 协议，从 Orin 的 7450 端口发出（Zenoh，不开组播）。车上怎么启用见 `runtime/nav-bridge/README.md` 和 DORA_NAV `apps/tracer/README.md`：用 `runtime/nav-bridge/install.sh /home/nvidia/nvme/dora-lab/opt/nav-bridge` 装一份，把它打印的 `NAV_VIZ_BRIDGE` / `NAV_VIZ_BRIDGE_CONFIG` 两行写进 `~/.config/dora-nav/env`。桥接不上时 `autostart.sh` 会退回不带桥的数据流，车照常起来。

在 Studio 里：

1. "3D visualization" → "Visualization" 标签：Mode 填 `client`，Endpoint 填 `tcp/<Orin 的 IP>:7450`，Prefix 保持 `dviz`，点 "Connect"。
   - Orin 的 Wi-Fi 地址由 DHCP 分配，会变（2026-11-23 是 `192.168.1.160`），在 Orin 上用 `ip -4 -br addr` 查。
   - 不要用组播模式连车：车上的 Dora 节点自己也在用 Zenoh 组播，会显示 "Connected" 却收不到 dviz 数据。
2. 需要 3D 时点 "Open 3D window"。
3. 切到 "Navigation" 标签看 2D 地图。没连上时这里的状态行写 "Not connected — set Mode and Endpoint on the Visualization tab and press Connect"（或 "Listening, no peer found yet — …"、带错误原因），连接设置只在 "Visualization" 标签里。

两个视图的内容（线和点的颜色是车上桥节点配置里的默认值，可在配置里改；Studio 没收到颜色时用同样的默认值）。2D 地图仿 RViz：画布深灰 `#303030`（浅色、深色主题都一样），上面铺 1 m 网格（`(160,160,164)`，不透明度 0.5），从下到上依次是栅格地图、代价地图、网格、规划、轨迹、激光、足迹、位姿箭头、目标。

| 实体 | 2D 地图 | Rerun 3D |
|---|---|---|
| `map/grid` 栅格地图 | RViz map 配色：空闲白、障碍黑、未知 `#708986`，不透明度 0.7（透出一点画布，空地呈浅灰） | 铺在地面上的贴图（Rerun 0.28 画贴图不管透明度，所以是不透明的）；另在 `<robot>/view/grid` 画 1 m 网格线，见表下 |
| `costmap/global` 全局代价地图（规划器有变化时 ≤ 1 Hz，每 5 s 补发） | RViz costmap 配色，叠在地图上，不透明度 0.5：空闲透明，膨胀区由蓝到红，内切青，致命品红，未知 `#708986`；图例里 "Costmap" 可关 | 比地图高 1 cm 的色块，同样的配色，空闲格不画；整块不透明度 0.5（网格体的 albedo 系数），见表下 |
| `map/cloud` PCD 地图（≤ 5 万点） | — | 浅灰点；默认不发（Displays 里默认不勾，勾上才发） |
| `robot/pose` 车的位姿 | 红色箭头 `(255,25,0)`，0.4 m 长（至少 16 px） | 0.3 m 坐标轴，加 0.6 m 红色箭头 `(255,25,0)`（`<pose>/heading`，线半径 0.04 m，抬高 0.05 m 离地，跟着位姿走）；状态里有 `robot_model` 时再加深灰车体模型（`<pose>/model`，见表下） |
| `robot/footprint` 足迹 | 浅灰 `(220,220,220)` 2 px 框，内部半透明深灰；定位不可信（`loc_ok` 为假）或定位状态过期（状态行写 "Localization (no data)"）时框变红 | 浅灰框，线半径 0.01 m |
| `nav/plan` 规划路径（最后一条每 2 s 重发一次，晚连上的也能看到；规划出错时清空） | 浅蓝 `(0,180,255)` 线，4 px | 浅蓝线，半径 0.025 m |
| `nav/trail` 实际轨迹（从当前目标开始，每 5 cm 一点；丢定位后续行重发的同一目标 `<id>-r<n>` 不清空） | 绿 `(0,220,0)` 线，3 px | 绿线，半径 0.02 m |
| `nav/goal` 当前目标 | 橙黄 `(255,170,0)` 圆环，半径 0.15 m（至少 10 px）；有朝向时加 0.5 m 箭头，没给朝向的只有圆环 | 橙黄箭头（没给朝向时竖直向上），加半径 0.2 m 的橙黄圆环（`<goal>/ring`，24 段），线半径（箭头和圆环）都是 0.04 m；目标取消时一起清掉 |
| `sensors/scan` 激光 | 红色 3 px 方点 | 红点 |
| `sensors/cloud` 实时点云（≈ 5000 点） | — | 按高度着色，点半径 0.01 m，不透明度 0.6（看得见车和线） |
| `nav/status` 导航状态 | 面板上方的状态行，超过 2 s 没更新会标 "Stale"；某一部分的输入停了 2 s 以上写成 "Gate (no data)" 这样；底盘写控制模式、电池和非零故障码，如 "Chassis RC, disarmed (boot) · 25.6 V · error 0x4" | — |

Rerun 里的网格、代价地图和车体模型：

- **网格**：收到第一张 `map/grid` 后，在 `<robot>/view/grid`（如 `tracer/view/grid`）记一组静态的 1 m 网格线，颜色 `(160,160,164)`、不透明度 0.5，线半径 3 mm，高 2 mm（在地图之上、代价地图之下），盖住地图范围（向外取整到米）。地图每 5 s 补发一次，范围没变就不重画。Rerun 3D 视图自带的无限网格照旧在；不改它的设置，因为 Rust SDK 0.28 没有方便的 blueprint 接口，发 blueprint 还会替换查看器自动排的布局。
- **代价地图**：每行相邻、颜色相同的非空闲格合成一个四边形，整张图做成一个顶点着色的 `Mesh3D`，静态记录在 z = 0.01 m。Rerun 0.28 不管顶点颜色和贴图里的透明度，所以 0.5 的不透明度来自网格体的 albedo 系数：白色 `(255,255,255)`、alpha 128（与 2D 代价地图相同），只调透明度、不改颜色；整张代价地图只有这一个透明度，正好对应 RViz 的 0.5。不用贴图，因为贴图里每格的透明度同样被忽略，空闲格就没法空出来，地图会被整个盖住。车库地图（187 × 208 格）有约 3.5 万个非空闲格（大部分是未知区），合成后约 8800 个四边形。超过 10 万个四边形的代价地图不画（之前那张留着），记一次 sink 错误。
- **车体模型**：桥节点在每条 `nav/status` 里带 `robot_model`（默认 `"tracer_mini"`，见 `runtime/nav-bridge/README.md`）。每次连接（点 "Connect"）后第一次收到它，Studio 就在 `<robot>/robot/pose/model`（如 `tracer/robot/pose/model`）静态记录插件自带的 Tracer Mini 车体（STL 的 `Asset3D`，深灰 `(64,64,64)`），挂在位姿下面跟着走；之后的状态不再重发。
  - 模型来自 tracer-mini-ros2 的 `base_link.STL`，用 `plugins/dviz/tools/make_tracer_model.py` 顶点聚类精简：85 014 → 19 581 个三角形，979 KB，编译时嵌进插件（只在带 `rerun` 特性的构建里），不走 Wi-Fi。包围盒的水平中心对准 `base_footprint` 的 (−0.11, 0)，最低点贴地（z = 0），不旋转、不缩放：网格的 x（较长的一边，0.43 m；y 是 0.42 m）当作车头方向。来源和许可证见 `plugins/dviz/assets/NOTICE-tracer-mini.md`。
  - 跟着 Displays 里的 `robot/pose`：关掉时模型和位姿一起清掉，再勾上马上重新记一次；位姿关着的时候收到的模型，等勾上再记。"Open 3D window" 换了查看器，也给新查看器记一次。
  - 不认识的名字什么都不画，只记一条警告日志；`robot_model` 为空（`null`）不画模型。从网上收到的 `model_ref` 消息不会送进 Rerun，模型只由 Studio 自己记。
  - 重新生成：`PYTHONNOUSERSITE=1 python3 plugins/dviz/tools/make_tracer_model.py <base_link.STL> plugins/dviz/assets/tracer_mini.stl`。只用 numpy；默认不超过 1 000 000 字节、中心 (−0.11, 0)、底面 z = 0（`--max-bytes`、`--center-x`、`--center-y`、`--floor-z` 可改）；同样的输入得到逐字节相同的输出；源网格的 x 或 y 方向尺寸不在 0.3–0.6 m 之间（单位不是米）就报错退出，不猜缩放。

Navigation 面板的操作：滚轮缩放（以光标为中心），拖动平移（会关掉跟随）；"Fit map" 适应地图，"Follow robot" 让车保持在中间；右侧图例每层一个开关（Map、Costmap、Robot、Plan、Trail、Goal、Scan，默认都开；Robot 管足迹和位姿箭头），色块是该层的颜色，Costmap 的色块是致命区的品红。足迹的填充最多取 8 个角点，角点更多的足迹只画框。工具栏从左到右是 "E-STOP"、"Disarm"、"Fit map"、"Follow robot"、"2D Pose Estimate"、"2D Goal Pose"：窗口窄时右边的按钮先被挤掉，E-STOP 和 Disarm 总在；这四个命令按钮见下面“命令”一节。Wi-Fi 上的急停不是安全装置，遥控器切 RC 才是。

日志、节点和数据流图来自车上各节点真实的 Dora 日志，在 "Visualization" 标签的 Logs / Nodes 里看。

**升级顺序（costmap）**：桥现在会发 `costmap/global`（`grid2d`，kind `costmap`）。交付 ① 的旧版 Studio 不认识这个层，会把任何 costmap 类网格画成不透明的未知色（灰）静态色块，盖在 Rerun 的地图上（z = 0），Displays 开关清不掉静态数据，5 s 内又会补发回来；2D 视图不受影响。所以请与桥同时升级 Studio，或先于桥升级。临时办法（旧 Studio 不能改）：把车上已安装的 `tracer.json` 的 `inputs` 里的 `"costmap"` 一行删掉，桥就不再发这个层（`inputs` 里没有的 Dora 输入会被忽略），改完重启桥。

## 命令（Navigation 面板）

Navigation 工具栏最左边是两个按钮 "E-STOP"、"Disarm"，右边是两个工具 "2D Pose Estimate"、"2D Goal Pose"。命令用 Zenoh 查询发给车上的桥节点（键 `<prefix>/<robot_id>/cmd`），桥节点检查后转给 DORA_NAV 的 nav_command，再把结果回给发命令的这台 Studio。车上的细节（检查顺序、结果代码、`dviz_cmd` 命令行工具）见 `runtime/nav-bridge/README.md` 的 "Command channel"。武装和解除急停只能在车上做（`tracer_ctl.py arm` / `release`），Studio 不能。

### 令牌

设初始位姿和发目标要带共享令牌；急停和撤防不要。车上和操作员的机器各放一份同样的令牌，路径都是 `~/.config/dviz/cmd_token`，权限 600。在车上生成（括号里是子 shell，不会改掉当前 shell 的 umask；umask 只管新建的文件，所以文件原来就有时由后面的 `chmod` 收紧权限）：

```sh
( umask 077; mkdir -p ~/.config/dviz; head -c 24 /dev/urandom | base64 > ~/.config/dviz/cmd_token ); chmod 600 ~/.config/dviz/cmd_token
```

再把这个文件复制到操作员机器的同一路径，权限同样是 600。两边比较前都去掉首尾空白（文件末尾的换行、Windows 的 CRLF 都没关系）。桥节点只在启动时读一次，换令牌要重启 dora-nav；Studio 每次点 "Connect" 时读，只放在内存里，不显示、不写日志。

### 在车上打开

车上默认只收急停和撤防。要收设初始位姿和发目标：

1. 放好车上的令牌文件。组和其他人不能有任何权限位（`mode & 0o077 == 0`，如 600、400）；有一位（哪怕只是组可读或可执行），桥节点就当作没有令牌。
2. 在已安装的配置（`install.sh` 装到的目录里的 `tracer.json`，如 `/home/nvidia/nvme/dora-lab/opt/nav-bridge/tracer.json`）里，把已有的 `"cmd"` 一项中的 `"goal_enabled": false` 改成 `true`。不要再加一个 `"cmd"`：文件里已经有这一项，重复的键会让配置读不进来，桥节点整个停用（什么都不发，命令也没有回复）。`install.sh` 每次都用仓库里的 `tracer.json`（`goal_enabled` 为 `false`）覆盖它，每装一次都要再改一次。
3. 重启 dora-nav（先问）。桥节点启动时写一行 `nav-bridge: goal/initialpose commands enabled`，或 `... closed: <原因>`。

打开以后 `nav/status` 里的 `cmd_goal_enabled` 为真，Studio 的两个工具才可用。

### 四个按钮

- **2D Pose Estimate**（设初始位姿）：点按钮，在图上车实际所在的位置按下，朝车头方向拖动，松开就发 `initialpose`。拖动不到 10 像素时不发送，原因行显示 "drag to set the heading" 2 秒，工具保持激活，可以重来：初始位姿的朝向对定位很关键，不猜。
- **2D Goal Pose**（发目标）：在目标位置按下，朝到点后要面对的方向拖动，松开就发 `goal`。拖动不到 10 像素（只是点一下）发不带朝向的目标，车到点后不调整朝向。目标编号就是请求编号（如 `s3-k9xq`），和状态行里的 "Goal s3-k9xq ACTIVE" 对得上。
- 两个工具共同的规矩：
  - 工具激活时按钮显示为选中，光标是十字；按住拖动定朝向，不平移地图；滚轮缩放、"Follow robot" 照常；
  - 拖动时在按下的点画一个圆点，拖够 10 像素后从它画一个指向拖动方向的箭头（3 像素宽，地图上至少 0.3 m）：目标用目标图层的颜色（橙黄），初始位姿用紫色 `(170,90,255)`；按下的点存的是地图坐标，视图跟着车动或缩放时它不漂；
  - 在地图外松手也照发，按松手的位置算朝向；点在栅格范围外也照发，由导航处理（规划失败时目标状态写 ABORTED 和原因）；
  - 发送后工具自动退出；按 Esc（激活工具或在图上按下后，地图拿到键盘焦点）或再点一次按钮取消，这次拖动作废、什么都不发；
  - 激活或拖到一半时工具变得不能用（比如拖到一半断了连接或状态过期），工具直接退出，这次不发；
  - 车被撤防或急停时 2D Goal Pose 不退出，照样发出，由桥节点拒收（回执红字 `not_armed` / `estop_active`）。
- **E-STOP**：让车急停（安全门锁住急停并撤防）。
- **Disarm**：让车撤防。
- E-STOP 和 Disarm 按下就发（不等松开，按下后把鼠标移开也已经发出；松开不再发，按一次发一次），永远可点，没有确认框，也没有键盘快捷键（按它们不会拿键盘焦点）；不要令牌，不受车上开关、限流和安全门状态影响，用 Zenoh 最高优先级发出，拥塞时不丢。它们发给这次连接里见过的每一个机器人编号（从 `<prefix>/<robot_id>/nav/status` 话题记下，断开或重连时清空），从没见过的车不会被停。

### 回执和原因行

状态行下面两行：

- **回执行**：最近一条命令，如 `Last command: goal s3-k9xq → ok: sent to nav_command  (2 s ago)`（不带原因的回复只写结果代码，如旧桥节点的 `→ ok  (2 s ago)`）。等回复时是 `sending…`；没连上或还没见过车时是 `not sent: not connected` / `not sent: no robot seen yet`；3 秒内没有回复是 `no reply (robot not reachable)`。发给多台车时每台一段，如 `tracer: ok: …; tracer_sim: …`。只要有一台的结果既不是 `ok` 也不是 `sending…`，整行就用红色。结果代码（`ok`、`not_armed`、`bad_token`、`disabled`、`rate_limited`……）的含义见桥节点 README。`ok` 只说明命令已送进导航数据流；目标的进展照旧看状态行里的目标状态。
- **原因行**：工具为什么不能用。两个都不能用时是 `Pose / goal tools off: <原因>`，只有 2D Goal Pose 不能用时是 `2D Goal Pose off: <原因>`，都能用时为空；两个都能点、但车会拒收目标时是红字 `Goals will be refused: <原因>`（见下表最后三行）。不能用的工具按钮是灰的，点了没反应。

按下表顺序，第一条符合的就是原因：

| 条件 | 2D Pose Estimate | 2D Goal Pose | 原因 |
|---|---|---|---|
| 没有连接 | 灰 | 灰 | `not connected` |
| 连上了，但还没收到任何车的 `nav/status` | 灰 | 灰 | `no robot seen yet` |
| 这次连接见过不止一个机器人编号 | 灰 | 灰 | `more than one robot on this connection` |
| 本机没有令牌（文件不存在、为空或读不了） | 灰 | 灰 | `no token (~/.config/dviz/cmd_token)` |
| 导航状态超过 2 秒没更新 | 灰 | 灰 | `nav status stale` |
| 车上没打开（`cmd_goal_enabled` 为假） | 灰 | 灰 | `goal / pose commands not enabled on the robot` |
| 安全门状态停了（状态行写 "Gate (no data)"） | — | 可点，原因行红字提示 | `Goals will be refused: gate state unknown` |
| 急停锁着 | — | 可点，原因行红字提示 | `Goals will be refused: e-stop latched (release it on the robot)` |
| 没武装 | — | 可点，原因行红字提示 | `Goals will be refused: robot not armed (arm it on the robot)` |

车没武装 / 急停 / 安全门状态未知时 2D Goal Pose 仍可点（和 RViz 一样），目标会被桥节点拒收（回执 `not_armed` / `estop_active` / `gate_unknown`，红字），设计上的安全保护不变。

提示（变灰或红字）只是提示，不是保护：桥节点对每条命令再检查一遍，撤防、急停或安全门状态未知时拒收目标（回执 `not_armed` / `estop_active` / `gate_unknown`）。设初始位姿不要求武装（它通常在武装前做）。

**升级顺序（命令）**：交付 ③ 的旧桥节点没有命令通道。新版 Studio 连它时两个工具是灰的（原因 `goal / pose commands not enabled on the robot`），E-STOP 和 Disarm 的回执是 `no reply (robot not reachable)`：车不会停。所以先在车上装新的桥节点（`install.sh` 后重启 dora-nav），再升级 Studio。

### 已知局限

- **令牌是明文**：存在磁盘上（权限 600），也明文放在每条设初始位姿和发目标的查询里（Zenoh TCP，没有 TLS）。它防误操作（第二台 Studio、连错车），防不了能抓包的人。
- **撤防和目标之间有 0.1–0.2 s 的空隙**：桥节点检查"已武装"之后，nav_command 要到下一个 100 ms 周期才把目标送进导航。恰好在这段时间里撤防，目标会留下来，直到 15 s 后被放弃；这期间有人在车上武装，车会朝这个目标开。在车上用 `tracer_ctl` 发目标也有同样的空隙；彻底消掉要改 DORA_NAV。
- **Wi-Fi 上的急停不是安全装置**：网络断了、Studio 卡了，它就发不出去。真要停车用遥控器（切 RC）。
- 别的 Studio 看不到这台 Studio 发的命令和回执（车的状态变化照样在各自的状态行里）。
- 连接多台车时两个工具不能用；E-STOP 和 Disarm 照样发给每一台。

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

1. 先装好 Rerun 0.28 查看器（`rerun-sdk==0.28.2`），再启动发布器（见上）。
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
- Studio 能设初始位姿、发目标、急停和撤防（见“命令”一节）；武装和解除急停只能在车上做，Studio 里没有这两个按钮。
- Rerun 查看器版本必须是 0.28。
- “3D window: open” 只说明启动那一刻连上了；之后关掉 Rerun 窗口，这一行不会变，此后的 3D 数据直接丢弃，再点“Open 3D window”会重新启动。
- 9876 端口上如果已经有进程在监听，Studio 当它是 Rerun 查看器直接连过去（和 Rerun SDK 自己的做法一样），版本只核对 `PATH` 里的 `rerun`。
- Rerun 里的车体模型按"网格的 x 朝车头"摆放，还没在实车画面上核对；不对时改用 URDF 里的方块车体和圆柱轮子。
- 地图原点带朝向（`map.yaml` 的 `origin` 第三个值不为 0）时，2D 视图里栅格地图按没有旋转画，和其他图层对不齐；现在用的地图原点朝向都是 0。
- "Visualization" 标签里 Mode / Endpoint / Prefix 三个输入框用的是样式表自带的输入框（白底深色字），不跟着浅色 / 深色主题变：跟着主题变的版本在桌面上点了打不进字（2026-11-23），先换回 P1 的做法。
