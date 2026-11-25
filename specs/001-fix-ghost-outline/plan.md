# Implementation Plan: 修复 3D 机器人视图的姿态残留轮廓

**Branch**: `001-fix-ghost-outline` | **Date**: 2026-11-24 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-fix-ghost-outline/spec.md`

## Summary

3D 机器人视图在相机或关节移动后，会在空白背景处留下"只有轮廓、内部为背景色"的历史痕迹。规格把问题定义为一条与实现无关的不变量：**每一帧的可见内容只能来自该帧的相机状态、关节姿态与视口尺寸**。

技术路线：在渲染器的合成阶段引入**颜色覆盖门控**——只有"本帧真的画过颜色"的像素才允许参与轮廓/凹陷计算，从语义上消灭"有深度、无颜色"的像素被当作表面来描边的可能；同时用一次设备端**读回审计**（`Texture::read_back`）量化"有深度无颜色"的像素数，把规格的 FR-005 变成可计数的断言。补丁落在仓库已有的第三方渲染器补丁副本上，并在 `NEXUS_PATCH.md` 留痕。

## Technical Context

**Language/Version**: Rust 2021（根 workspace）；本机 1.98.0，CI 1.95.0（不使用更高版本语法）

**Primary Dependencies**: Makepad（git rev `44d8220f`，含 script-DSL 着色器）；`makepad-urdf-player` 本地补丁副本（源自 `283d303`，经 `[patch."https://github.com/dorobot-org/makepad-urdf-viewer"]` 指向 `vendor/`）；`glam`

**Storage**: N/A（无持久化；审计输出仅写日志/可选 JSON 文件）

**Testing**: `cargo test --locked --workspace --no-default-features`（CI 同款）+ 新增纯逻辑单元测试（残留计数）+ 设备端读回审计（开发期，需 GPU 上下文，不进 CI）

**Target Platform**: 桌面端，macOS/Metal 为首要验证目标；Linux/Vulkan 必须保持可编译；WebAssembly 变体不在本次范围

**Project Type**: 桌面应用 + 内置第三方渲染组件（`vendor/`）

**Performance Goals**: 交互 60 fps 档位；连续拖动视角 10 秒的平均帧时间 ≤ 修复前基线的 105%（SC-005）

**Constraints**: 不升级任何锁定依赖；默认行为零变化（所有诊断入口默认关闭）；保留 SSAA 2× 与现有描边/凹陷观感；不新增 crate；不改变模型加载与关节映射

**Scale/Scope**: 5 个使用 3D 视图的界面（机器人设置 / 操作 / 实时镜像 / 回放（含独立窗口） / 评估）；单个视口，单实例

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

`.specify/memory/constitution.md` 目前仍是未批准的模板，没有正式条款可对照。因此按仓库既有约定（`CONTRIBUTING.md`、`docs/PLANS` 系列计划文档、`AGENTS` 约定）临时设立以下门禁，待 constitution 批准后复核：

| # | 临时门禁 | 本计划是否满足 |
| --- | --- | --- |
| G1 | 先复现/先写失败测试，再改实现 | 满足：Phase 0 产出审计基线（残留像素计数 > 0），修复后同口径归零 |
| G2 | 不升级锁定依赖、不改无关第三方代码 | 满足：只改 `vendor/makepad-urdf-player` 的两处渲染路径与补丁说明 |
| G3 | 保持中英双语文档并存 | 满足：新增文档为中文，术语保留英文原名 |
| G4 | 不跨越插件/后端边界 | 满足：改动全部位于 3D 渲染组件内部，`Backend`、`Intent`、插件契约不动 |
| G5 | 硬件/真机行为不回归 | 满足：不触碰 `runtime/`、串口、Zenodo 遥测路径 |

**结论**：无违规项，无需 Complexity Tracking。

## Project Structure

### Documentation (this feature)

```text
specs/001-fix-ghost-outline/
├── plan.md              # 本文件
├── spec.md              # 规格（已完成）
├── research.md          # Phase 0：根因定位与方案取舍
├── data-model.md        # Phase 1：帧状态 / 覆盖 / 残留审计的数据模型
├── quickstart.md        # Phase 1：可执行的验收步骤
├── contracts/           # Phase 1：不变量、诊断入口、渲染目标生命周期的契约
│   ├── visual-invariant.md
│   ├── debug-hooks.md
│   └── render-lifecycle.md
├── checklists/
│   └── requirements.md  # 规格质量清单（已完成）
└── tasks.md             # Phase 2 输出（由 /speckit-tasks 生成）
```

### Source Code (repository root)

```text
vendor/makepad-urdf-player/
├── src/render/draw.rs        # 合成着色器：轮廓/凹陷项（门控在此）
├── src/render/audit.rs       # 新增：残留判定的纯逻辑（可单测）
├── src/robot_view.rs         # 3D pass 的尺寸、清除标志、读回审计的挂点
└── NEXUS_PATCH.md            # 记录本次补丁与上游回馈计划

crates/dorobot-studio-core/   # 不改（仅由 viewer 组件消费）
plugins/                      # 不改
dorobot-flex/                 # 不改
```

**Structure Decision**: 采用"就地补丁第三方副本"结构。残留发生在 `vendor/makepad-urdf-player` 的渲染管线内部（合成着色器消费深度、pass 生命周期管理），在调用方（`crates/dorobot-studio-core` 的 `RobotViewer` 包装）无法可靠规避，且调用方规避会掩盖根因而非修复。因此改动集中在 vendored crate，并以 `NEXUS_PATCH.md` + 独立 `audit.rs` 模块保持可回馈上游的形态。

## Phase 0 研究结论（详见 research.md）

1. **机制判定**：痕迹的像素特征（线宽 1–2 px、内部与背景差 ≈ 0、轮廓压暗 5–10 级）只与"深度存在、颜色为空"一致；渲染器把描边建立在深度上，且背景在合成阶段程序化生成，因此深度残留不会以"整块旧画面"出现，只以轮廓出现。
2. **修法取舍**：以"颜色覆盖门控"为主修（语义正确、代价一次乘法、与根因无关地覆盖整类问题）；生命周期修复作为第二层，仅在审计证明残留来自跨帧累积时启用。
3. **可验证性**：`Texture::read_back` 提供了设备端读回能力，可把"残留像素数"从目测变成计数；由于 macOS 屏幕录制权限缺失，自动化截图不可用，故采用"读回审计 + 人工基线截图"组合。
4. **补丁归属**：本地补丁副本，`NEXUS_PATCH.md` 记录；上游回馈 `dorobot-org/makepad-urdf-viewer` 作为后续独立事项。

## Phase 1 设计产物

- [data-model.md](./data-model.md)：帧状态、帧附件、覆盖掩码、残留候选、审计报告
- [contracts/visual-invariant.md](./contracts/visual-invariant.md)：FR-005 的可计数形式
- [contracts/debug-hooks.md](./contracts/debug-hooks.md)：诊断入口的默认值与输出格式
- [contracts/render-lifecycle.md](./contracts/render-lifecycle.md)：3D pass 的尺寸/清除/重录制契约
- [quickstart.md](./quickstart.md)：复现、审计、五界面人工清单、性能对比、回滚开关

## 风险与缓解

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 根因判定错误（选 A 实为 B） | 主修仍能消除症状，但可能掩盖真实缺陷 | 审计同时输出"无颜色但有深度"的像素**簇形状**（位置+面积），若形状与历史姿态一致则判 A，若与当前姿态重合则判 B |
| 门控阈值过严，吃掉真实抗锯齿边缘 | 描边变细/断裂 | 阈值取"覆盖 ≥ 1/4 像素"（与 SSAA 2× 的 4 子样本对齐），并用 SC-004 的基线差分守住 |
| 读回审计拖慢交互 | 验收期掉帧，污染性能数据 | 审计默认关闭；开启时只在指定帧执行，且性能测量在审计关闭时进行 |
| Makepad 内部字段（`paint_dirty`）不可公开访问 | 生命周期修复无法实施 | 回退到"尺寸变化时重建附件纹理"的公开 API 方案 |
| 上游同步冲掉补丁 | 修复丢失 | `NEXUS_PATCH.md` 增条目；后续以 PR 形式提交上游 |

## Post-Design Constitution Re-check

Phase 1 完成后复核 G1–G5：

| # | 复核结果 |
| --- | --- |
| G1 | 仍满足：`quickstart.md` §1–§2 要求先复现并取得 `RESIDUE` 基线，才进入修复 |
| G2 | 仍满足：设计未新增依赖，未修改 `Cargo.toml` 的锁定项 |
| G3 | 仍满足：文档中文、术语保留英文原名 |
| G4 | 仍满足：改动面限定在 `vendor/makepad-urdf-player` 内部，`Backend`/`Intent`/插件契约零改动 |
| G5 | 仍满足：未触及 `runtime/`、串口、遥测路径 |

新增设计没有引入违规项，`Complexity Tracking` 保持为空。两个诊断入口的默认值（`URDF_GHOST_GUARD` 开启、`URDF_GHOST_AUDIT` 关闭）符合 FR-008 与 FR-010。
