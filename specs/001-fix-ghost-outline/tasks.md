# Tasks: 修复 3D 机器人视图的姿态残留轮廓

**Input**: Design documents from `/specs/001-fix-ghost-outline/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: 本特性要求测试先行（plan.md 门禁 G1、research.md R4）。`cargo test` 能覆盖的部分必须是"先红后绿"；GPU 侧无法进 CI 的部分用审计输出的判定代替断言。

**Organization**: 任务按用户故事分组，每个故事都能单独实现、单独验证、单独交付。

## Format: `[ID] [P?] [Story] Description`

- **[P]**: 可并行（不同文件、无未完成依赖）
- **[Story]**: 所属用户故事（US1 / US2 / US3）
- 每条任务都带确切的文件路径

## Path Conventions

改动面集中在 vendored 渲染组件与本文档目录：

- `vendor/makepad-urdf-player/src/render/` — 合成着色器、新审计模块
- `vendor/makepad-urdf-player/src/robot_view.rs` — pass 生命周期、读回挂点
- `vendor/makepad-urdf-player/NEXUS_PATCH.md` — 补丁留痕
- `specs/001-fix-ghost-outline/` — 设计产物与本任务表
- `docs/ux/` — 视觉基线截图

---

## Phase 1: Setup

- [X] T001 记录基线：在仓库根运行 `cargo build --locked -p dorobot-flex --no-default-features` 与 `cargo test --locked --workspace --no-default-features`，把通过/失败数与耗时记到 `specs/001-fix-ghost-outline/tasks.md` 末尾「执行记录」
- [X] T002 [P] 在 `vendor/makepad-urdf-player/src/render/audit.rs` 建立模块骨架（仅类型与空函数），并在 `vendor/makepad-urdf-player/src/render/mod.rs` 声明；保证 workspace 仍可编译

---

## Phase 2: Foundational（阻塞所有用户故事）

**目标**：先具备"把残留数出来"的能力。没有它，后续所有验收都是目测。

- [X] T003 [P] 为残留判定写**失败**单测：`vendor/makepad-urdf-player/src/render/audit.rs` 中的 `residue_at(depth, coverage, gate)`、`classify(clusters, current_pose_bbox)`；断言 `data-model.md` 的 `Coverage.gate_threshold`（超采样 2× 时为 0.25）与 `ResidueAudit.verdict` / `ResidueAudit.hypothesis` 的全部枚举取值（`CLEAN|RESIDUE`、`HISTORIC_POSE|CURRENT_POSE|UNKNOWN`）
- [X] T004 实现审计纯逻辑并通过 T003：像素计数、8 邻域聚类、`largest_area`、`verdict` / `hypothesis` 判定（字段定义见 `data-model.md` 的 `ResidueAudit`）
- [X] T005 [P]（内核完成）在 `vendor/makepad-urdf-player/src/robot_view.rs` 实现启动期读取 `URDF_GHOST_GUARD` 与 `URDF_GHOST_AUDIT`（默认：门控开启、审计关闭），未设置时不得新增任何绘制/读回/分配（契约：`contracts/debug-hooks.md`）
- [ ] T006（**受阻**，方案待定）在 `vendor/makepad-urdf-player/src/robot_view.rs` 建**审计采集管线**（读回只支持 `RenderBGRAu8`，深度无法直接读回，见「执行记录 · 实施修正」）：①全分辨率把"残留掩码"写入一张 BGRA8 目标；②用 16×16 循环下采样成计数块写入小目标；③按 `contracts/debug-hooks.md` schema 打印单行 JSON。仅 `URDF_GHOST_AUDIT=1` 时执行，进程内至多 3 次（字段：`data-model.md` 的 `ResidueAudit.frame`/`pass_size`/`candidates`/`clusters`/`largest_area`）
- [ ] T007 取得修复前基线（门禁 G1）：按 `quickstart.md` §1–§2 复现并记录一条 `"verdict":"RESIDUE"`、`candidates>0` 的审计输出，连同 `hypothesis` 写入本文件末尾「执行记录」

---

## Phase 3: User Story 1 - 操作后画面只呈现当前状态 (P1)

**Goal**: 视角或关节停止移动后，画面只反映当前状态；空白背景零残留。
**Independent Test**: 视角 10 次 + 关节 10 次后审计为 `CLEAN`，且目视无"内部为背景色"的轮廓。

- [X] T008 [US1] 为门控写**失败**单测：把"覆盖不足时不压暗"抽成纯函数并测试（`coverage ≤ gate ⇒ shade = 1.0`；`coverage > gate ⇒ 与旧公式一致`）；约束见 `data-model.md` `Coverage.gate_threshold` 与 `contracts/visual-invariant.md` §1
- [X] T009 [US1] 在 `vendor/makepad-urdf-player/src/render/draw.rs` 的两处 `DrawSceneComposite`（native 与 wasm 变体，约 591–630 行与 690–720 行）加入覆盖门控；新增 `coverage_gate` uniform，取值规则见 `data-model.md` `Coverage.gate_threshold`
- [X] T010 [US1] 在 `vendor/makepad-urdf-player/src/robot_view.rs` 传入 `coverage_gate = 1 / ssaa_scale²`（`FrameAttachments.ssaa_scale`，见 `data-model.md`），并接上 `URDF_GHOST_GUARD=0` 的关闭分支（契约：`contracts/debug-hooks.md`）
- [ ] T011 [US1] 验收：`URDF_GHOST_AUDIT=1` 下按 `specs/001-fix-ghost-outline/quickstart.md` §1 执行操作，审计必须为 `CLEAN`；若仍为 `RESIDUE`，记录 `hypothesis` 并在 T014 触发对应分支
- [ ] T012 [US1] 回归：`cargo test --locked --workspace --no-default-features` 全绿；确认 `URDF_GHOST_GUARD=0` 能复现旧行为（证明修复来自门控，而非环境差异）

---

## Phase 4: User Story 2 - 视口尺寸变化后不残留 (P2)

**Goal**: 缩放窗口、拖拽分隔条、最大化/还原、DPI 变化后，新视口范围内无旧尺寸的画面片段。
**Independent Test**: 尺寸变化各 5 次后审计为 `CLEAN`；审计中 `pass_size` 与当帧视口尺寸一致。

- [ ] T013 [US2] 为尺寸派生写**失败**断言：`pass_size` 必须由当帧 `viewport_size` 派生（`data-model.md` `FrameState.pass_size`，不变式 I-1），用日志或纯函数测试固定该行为
- [ ] T014 [US2] 按 T011/T007 的 `hypothesis` 分支处理：`HISTORIC_POSE` 时落实 `contracts/render-lifecycle.md` §2/§3（尺寸变化重建附件或每帧无条件清零，且不复用含旧姿态的录制）；`CLEAN` 时只补充"尺寸变化必须重录制"的断言与日志，不改 pass 生命周期
- [ ] T015 [US2] 验收：窗口缩放与分隔条拖动各 5 次后审计 `CLEAN`；同时完成 `specs/001-fix-ghost-outline/quickstart.md` §4 表格中第 1、4 个界面的"尺寸"列

---

## Phase 5: User Story 3 - 修复不牺牲视觉质量与流畅度 (P3)

**Goal**: 描边与凹陷阴影仍在、位置不变；交互帧时间不退化。
**Independent Test**: 与修复前基线截图对比 ≤2%；连续拖动 10 秒的帧时间 ≤105%。

- [ ] T016 [US3] 视觉基线：用 `URDF_GHOST_GUARD=0` 在固定相机角度与零位姿态截图存为 `docs/ux/baseline-ghost-fix.png`；修复后同条件再截一张并比较，判定标准见 `data-model.md` `BaselineRecord.diff_ratio`（≤ 0.02）与 `contracts/visual-invariant.md` §3
- [ ] T017 [US3] 性能：审计关闭状态下连续拖动视角 10 秒，记录平均与 p95 帧时间，与 T001/T016 的基线比较（阈值见 spec.md SC-005）
- [ ] T018 [US3] 开关正交性（验证见 `specs/001-fix-ghost-outline/quickstart.md` §3.2）：`URDF_EDGE=0`、`URDF_AO=0` 两种组合下审计均为 `CLEAN`（FR-010）；确认关掉效果时"无描边可残留"、开启时"门控生效"

---

## Phase 6: Polish & Cross-Cutting

- [ ] T019 在 `vendor/makepad-urdf-player/NEXUS_PATCH.md` 增加本次补丁条目：改动点（覆盖门控 + 审计）、验证证据、上游回馈计划与移除条件（见 `contracts/debug-hooks.md` §5）
- [ ] T020 [P] 把实测结果回填 `specs/001-fix-ghost-outline/quickstart.md`（审计输出原文、基线截图路径、性能数字），并在 `docs/` 增一节"如何审计 3D 视图残留"（中英文术语并存）
- [ ] T021 [P] 清理：确认默认路径无新增日志、无默认读回、无遗留调试打印；`git diff` 只含预期文件（`vendor/makepad-urdf-player/` 下 3 个文件 + 文档）
- [ ] T022 最终验证：`cargo test --locked --workspace --no-default-features` 全绿 + 三个审计场景（默认 / `URDF_EDGE=0` / `URDF_GHOST_GUARD=0`）+ `specs/001-fix-ghost-outline/quickstart.md` §4 的 15 个格子全部勾选

---

## Dependencies

```text
Phase 1 (T001,T002)
      │
Phase 2 (T003→T004, T005, T006, T007)      ← 必须先有"数得清"的手段
      │
Phase 3 US1 (T008→T009→T010→T011→T012)     ← MVP 边界
      │
      ├── Phase 4 US2 (T013→T014→T015)      ← T014 依赖 T011 的 hypothesis
      │
      └── Phase 5 US3 (T016, T017, T018)    ← 依赖 T010（门控已存在）才谈"不退化"
             │
      Phase 6 Polish (T019, T020, T021, T022)
```

**Story 之间的独立性**：US1 完成后，US2 与 US3 可分别进行、互不阻塞（US2 改 pass 生命周期，US3 只做测量与截图）。US3 的 T016 需要"修复前"的截图，若 T014 已改动 pass，请用 `URDF_GHOST_GUARD=0` 取基线，不要回退代码。

## Parallel Execution Examples

**Phase 2 内**：T003（失败单测）与 T005（读取开关）可并行，文件不同；T004 必须等 T003 红；T006 必须等 T004+T005。

```text
并行组 A：T003（audit.rs 测试） + T005（robot_view.rs 开关）
串行：T003 → T004 → T006 → T007
```

**Phase 6 内**：T020（文档回填）与 T021（清理检查）可并行；T019 与 T022 串行收尾。

## Implementation Strategy

**MVP = Phase 1 + Phase 2 + US1（T001–T012）**。交付后可验证：视角/关节操作后审计归零、`URDF_GHOST_GUARD=0` 能复现旧行为。这已经解决用户报告的问题。

**增量交付顺序**：
1. 先交"能量化"的基础设施（T001–T007）——它的价值独立于修复：以后任何 3D 渲染回归都能用它判定。
2. 再交门控（T008–T012）——最小可用修复。
3. US2 只在审计给出 `HISTORIC_POSE` 时才是"修根因"；否则只是补强断言，不要为了对称而改动 pass 生命周期。
4. US3 是放行门槛，不是可选项：视觉退化与掉帧都不接受。

**明确不做**：不在本特性内重构渲染管线、不新增后处理、不改 `crates/dorobot-studio-core` 的 `RobotViewer` 接口、不升级任何锁定依赖。

---

## 执行记录

### T001 基线（2026-11-24）

- `cargo build --locked -p dorobot-flex --no-default-features`：**通过**（增量 5.4 s）。
- `cargo test --locked --workspace --no-default-features`：**1 个既存失败**，与本次修复无关：
  `crates/dorobot-studio-core/tests/nexus.rs:125`
  `Some("/private/var/…/model")` vs `Some("/var/…/model")` —— macOS 上 `/var` → `/private/var` 的符号链接导致路径字符串比较失败。记录待办，不在本特性内修。
- 其余测试通过。基线记为"1 项既存失败"。

### 实施修正（T002–T005 期间发现，设计已回写）

1. **深度无法通过 `read_back` 读回**：`Texture::read_back` 只接受 `TextureFormat::RenderBGRAu8`，`DepthD32` 直接返回 `UnsupportedFormat`。`contracts/debug-hooks.md` §2 因此改为"两段式审计采集"（掩码 → 下采样计数 → 读回小目标）。原 R4 中"直接读回颜色+深度"的写法作废。
2. **vendored crate 自带 `[workspace]`**：`cargo test --workspace` **不会**运行 `vendor/makepad-urdf-player` 内部的 `#[cfg(test)]`。因此纯逻辑测试放在 `dorobot-flex/tests/ghost_audit.rs`（该 crate 依赖 viewer，可正常链接），CI 能覆盖。

### T006 受阻（2026-11-24）

已实现：`DrawGhostAudit` 着色器 + 结构体（`render/draw.rs`）、审计离屏 pass 与掩码绘制、`Texture::read_back` 请求、结果收口与 JSON 打印（`robot_view.rs`）。

运行证据：

```
URDF_GHOST_AUDIT armed
URDF_GHOST_AUDIT request queued (frame 1)     ← 请求已发出
（此后 14 秒无结果，进程 47% CPU 说明仍在重绘）
```

早前用 `next_render: false` 时拿到 `NotRendered`；该错误的分支条件是 `producer_serial == 0`，即**审计 pass 从未真正产出过这张纹理**。因此问题不在读回等待，而在掩码 pass 没有被提交渲染。

待验证的假设（按可能性）：
1. `cx.make_child_pass` 之后 `begin_pass/end_pass` 的 pass 在 2D draw list 里没有形成有效绘制（`draw_abs` 未产生 instance，或 pass 的 paint_dirty 从未置位）。
2. 审计 pass 的 `set_size` 被 `begin_pass` 对子 pass 的 `pass_rect` 覆盖，附件尺寸与预期不符。
3. 读回需要 `Event::Signal` 泵，而当前重绘循环不产生该事件。

已排除的三种尝试（均在 `--screen play` 下运行 15 秒、审计只发一次请求）：

| 尝试 | 结果 |
| --- | --- |
| `Auto` 尺寸纹理 + `next_render:false` | `NotRendered`（`producer_serial == 0` → pass 从未产出） |
| `Auto` 尺寸 + `next_render:true` | 请求挂起，永不完成；进程 47% CPU 说明重绘在继续 |
| `Fixed` 尺寸 + 显式 `paint_dirty` + `dpi_factor` | 同上，仍无结果 |

**判断**：问题不在读回参数，而在"审计 pass 里的 2D `draw_abs` 没有为该 pass 生成有效绘制"。框架内可参考的可靠样本是 `platform/src/cx_api.rs` 的 `readback_routing_tests::pending_target()`：`Fixed` 尺寸 + `add_color_texture` + `dpi_factor` + 开画前置脏，且绘制走 draw list 而非 2D `draw_abs`。

**已回退**：为避免把不可用的实验代码留在主路径上，`robot_view.rs` 与 `render/draw.rs` 已恢复到 HEAD；保留 `render/audit.rs`、模块声明与 7 个契约测试（默认零行为变化）。

**下一步选项**：
1. 按 `pending_target()` 的形态重写审计 pass（改用 draw list + `draw_vars.draw`，或复用 `draw_scene` 的 3D 路径），时间盒 1 次尝试。
2. 先做 T008–T012（覆盖门控 = 真正的修复），用人工截图验收；审计作为后续增强，不阻塞修复。

**建议**：选 2。修复与审计相互独立，门控本身不依赖审计；审计只是把"看得见"变成"数得清"，可以后补。

### T008–T010 结果（2026-11-24）

- T008：新增 `shade_with_coverage(shade, coverage, gate)` 纯函数 + 契约测试，`ghost_audit` 现为 **8 passed**。
- T009：合成着色器的两处（native / wasm）`onbody` 改为
  `step(dc, 1-ε) * step(scene.w, coverage_gate)`；`DrawSceneComposite` 新增 `coverage_gate`（默认 0.25）。
- T010：`RobotView` 每帧写入 `coverage_gate = 1/ssaa²`（门控开）或 `0`（`URDF_GHOST_GUARD=0`）。
- 构建通过，`--screen play` 启动无脚本错误；等待 T011 的人工 A/B 结论。

### T003/T004 结果

`cargo test -p dorobot-flex --no-default-features --test ghost_audit` → **7 passed**（门限、判定、聚类、CLEAN、CURRENT_POSE、JSON 契约、开关默认值）。
