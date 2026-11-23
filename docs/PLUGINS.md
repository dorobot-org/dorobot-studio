# Rust 插件工作台

此分支验证 RViz 风格的原生扩展边界：现有界面由独立 Rust crate 提供，宿主通过工厂创建实例，负责挂载、导航和事件分发。当前插件与宿主一起编译；不提供动态库安装、热卸载或跨版本 Rust ABI。

## 分层与依赖

```text
dorobot-flex                         宿主、导航、通用工作流
  ├── dorobot-studio-plugin          插件契约、工厂注册表、面板生命周期
  ├── dorobot-plugin-collection      录制、审阅与保存
  ├── dorobot-plugin-control         机器人设置、校准与实时操作
  ├── dorobot-plugin-dviz            dviz 3D 可视化与数据流图（见 [DVIZ.md](DVIZ.md)）
  └── dorobot-plugin-policy          本机 ACT 训练与 Nexus 推理
        └── dorobot-studio-core      业务后端、数据格式、回放与共享控件
```

`dorobot-studio-core` 提供共享的 `Backend`，负责真实设备、Dora、Nexus 会话和录制数据。插件各自依赖共享接口。界面构造时仅加载控件。

原 `dorobot-flex` 库保留兼容导出，现有启动和示例命令继续可用。共享实现移到 `crates/dorobot-studio-core/src/`；实际采集和设置界面分别位于 `plugins/collection/src/screen.rs` 和 `plugins/control/src/screen.rs`。

## 运行与观察

```sh
cargo run --locked -p dorobot-flex --no-default-features -- --demo
cargo run --locked -p dorobot-flex --no-default-features
```

侧栏由插件清单生成，每个插件只占一个名称入口。点击“机器人控制”后，右侧顶部显示“机器人设置／操作”标签；“数据采集”显示“录制”标签。切换回来会恢复本次会话最后使用的标签，快捷键及业务跳转也同步选中状态。这些界面由注册表的工厂创建，宿主不在自己的界面脚本中声明它们。

“数据工作区”是单独的内置入口，顶部提供“数据集／回放／评估”标签；这些页面本轮仍由宿主管理。侧栏不再重复列出插件的子功能。

默认视频构建需要 FFmpeg 开发库；无视频构建仍可验证插件界面和数据工作流。真实设备与原生仿真的依赖保持原有要求。

### SO-101 Nexus 采集、训练和推理

在 Linux Vulkan 环境中构建 [Nexus 运行时](../runtime/README.md)，然后启动带视频支持的 Studio。Python 选用安装了 LeRobot 及其训练依赖的本地环境。

```sh
cargo build --manifest-path runtime/Cargo.toml --release --locked --bin dorobot-so101-session
DOROBOT_LEROBOT_PYTHON=/path/to/lerobot/.venv/bin/python \
  cargo run --locked -p dorobot-flex --features video -- --screen nexus-setup
```

1. 在「机器人控制 → SO-101 Nexus」选择实体主臂串口和已有的标定 JSON，再启动采集。验证界面可用 `DOROBOT_NEXUS_SYNTHETIC=1`，保存的片段会标记合成输入。
2. 在「数据采集」重置方块和托盘，等页面提示可以录制再开始。结束后审阅并保存或丢弃本段。停止会话后可以打开已保存片段。
3. 在「数据工作区」回看或排除片段，再点「用于训练」。也可在训练页填写包含多个 LeRobot v3 数据集的目录。训练会记录数据来源和所选片段。
4. 训练完成后用「在 Nexus 中运行检查点」进入推理。运行结果包含相机、六轴读回与目标、H.264 视频和独立的抓放判定。

## 开发者接口

`dorobot-studio-plugin` 提供：

- `StudioPlugin`：描述、注册 Makepad 类型、按面板类型创建独立实例、检查是否允许禁用。
- `PluginDescriptor`：全局插件 ID、标题、SDK API 版本与面板清单。
- `PanelDescriptor`：面板类型 ID、标题、可选的旧页面映射。新工具可以将映射设为 `None`，由工具箱直接打开。
- `PluginPanel`：实例的 `WidgetRef`、状态同步、事件处理、可见性策略和停用回调。
- `PanelContext`：宿主提供的只读业务状态、实时视图可用条件与展示选项；事件处理通过可变 `Backend` 提交业务意图。

初始化顺序为共享控件注册、插件类型注册、界面工厂创建、容器挂载。宿主轮询后端，向当前可见实例同步快照，再将界面事件交给实例处理。离开面板触发 `on_disable`，保留实例以便重新进入；当前没有运行时删除插件库的机制。

注册表拒绝重复插件、冲突面板和不匹配的 API 版本。工厂描述不保存面板状态；多次调用 `create_instance` 获得不同界面。采集插件在录制、保存或待审阅期间拒绝禁用；控制插件在活动控制会话期间拒绝禁用。当前工具箱仅提供打开入口，禁用接口用于宿主集成和契约测试。

宿主使用 `mount_toolbox` 挂载插件分组、`mount_tabs` 挂载面板标签，再通过 `sync_navigation` 同步当前页面。两种按钮都从描述生成；新增插件不需要单独编写导航入口。`toolbox_selection` 恢复上次标签，`tab_selection` 仅接受当前可见标签的事件。

需要显示机器人模型的插件使用 `dorobot_studio_core::data::robot_model::RobotModelLoader` 在后台解析 URDF/STL，通过 `Signal` 或状态同步接收最新结果，再在界面线程调用 `RobotView::set_robot`。挂载前恢复最新关节角和场景相机，避免首帧跳回零姿态。加载器合并连续选择、忽略过期结果，并缓存最近两个未修改的模型；Makepad 控件和 GPU 操作仍留在界面线程。

## 添加插件

1. 在独立目录建立 Rust crate，依赖 `dorobot-studio-plugin`，需要业务服务时依赖 `dorobot-studio-core`。统一使用工作区固定的 Makepad 修订。
2. 实现 `StudioPlugin` 与面板实例；用 `script_eval!`、`WidgetRef::script_from_value` 创建界面。可参考内置插件。
3. 使用全局唯一插件 ID 和面板 ID。自有面板状态放在实例中，通过 `Backend` 访问共享业务，不复制设备驱动和会话管理。
4. 将 crate 加入工作区与宿主依赖，只在 `dorobot-flex/src/plugins.rs` 组合入口注册工厂。新增普通面板无需修改 `App` 的导航分支或现有 `Screen` 枚举。
5. 加入实例独立性、实际按钮路由及活动任务保护测试。

当前保留的通用工作流工具栏、文件选择器、回放窗口和硬件显示探针仍由宿主管理。全部迁移到插件、动态场景扩展点、工作区配置迁移和独立二进制分发属于后续工作；此验证版没有声称这些已经完成。

## 验证

```sh
cargo test --workspace --no-default-features
cargo test -p dorobot-flex --no-default-features --test plugins
cargo test -p dorobot-studio-plugin --test registry
cargo build --locked -p dorobot-flex --no-default-features
```

`plugins` 集成测试创建真实 Makepad 控件，对采集按钮发出事件并核对现有后端的录制、审阅与保存状态；测试不会打开真实硬件。注册表测试覆盖注册冲突、未知面板及启停状态。
