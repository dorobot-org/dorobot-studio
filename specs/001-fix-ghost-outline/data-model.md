# Phase 1 数据模型：帧状态、覆盖与残留审计

**Feature**: 001-fix-ghost-outline | **Date**: 2026-11-24

本特性不引入持久化数据；"数据"指一帧渲染过程中被读取、派生和判定的量。定义它们的目的是让规格中的 FR/SEC 可以被逐条映射到可计算的对象上。

---

## 1. FrameState（帧状态）

一帧应当呈现什么，由这三组值唯一确定；规格 FR-001 即"可见内容 ∈ f(FrameState)"。

| 字段 | 类型 | 说明 | 来源 |
| --- | --- | --- | --- |
| `frame_index` | u64 | 单调递增帧号，仅用于审计与日志 | 渲染循环 |
| `camera_view` | Mat4 | 相机视图矩阵 | 交互 / 动画 |
| `camera_projection` | Mat4 | 投影矩阵（含视口宽高比） | 视口尺寸 |
| `joint_angles` | Vec\<f32\> | 按 URDF 关节序的姿态 | 手动 / 回放 / 遥测 |
| `viewport_size` | Vec2 | 视口在界面上的逻辑尺寸 | 布局 |
| `pass_size` | Vec2 | 离屏附件像素尺寸 = `viewport_size × SSAA_SCALE` | 派生 |

**不变式**

- **I-1**：`pass_size` 必须由当帧 `viewport_size` 派生，不得沿用上一帧的尺寸（对应 FR-004）。
- **I-2**：`joint_angles` 的变更必须在同一帧内反映到可见内容（对应 FR-003），不允许"下一帧才追上"。

## 2. FrameAttachments（帧附件）

| 字段 | 类型 | 清除值 | 说明 |
| --- | --- | --- | --- |
| `color` | RGBA8（超采样） | `(0,0,0,0)` | 无几何时 alpha = 0；这是"覆盖"的唯一来源 |
| `depth` | D32 | `1.0` | 无几何时为 1.0；这是"存在性"的唯一来源 |
| `ssaa_scale` | f32 | `2.0` | 合成阶段四点平均的子样本数 = `ssaa_scale²` |

**不变式**

- **I-3**：任一帧的附件必须在绘制几何之前被完整清零；"部分清零"属于违约（对应 FR-001/FR-004）。
- **I-4**：`color.a` 与 `depth` 的一致性：`depth < 1-ε` 的像素若 `color.a = 0`，该像素即为**残留候选**（见 §4）。

## 3. Coverage（覆盖）

| 字段 | 类型 | 取值 | 说明 |
| --- | --- | --- | --- |
| `raw_alpha` | f32 | [0,1] | 合成阶段采样到的颜色 alpha（已含超采样平均） |
| `coverage` | f32 | [0,1] | 同上；命名用于表达"该像素被本帧几何覆盖的比例" |
| `gate_threshold` | f32 | `1 / ssaa_scale²` = 0.25 | 参与描边/凹陷的最低覆盖 |

**不变式**

- **I-5**（门控）：当 `coverage ≤ gate_threshold` 时，描边与凹陷项 MUST 输出 1.0（即不压暗），无论深度如何。
- **I-6**（保真）：当 `coverage > gate_threshold` 时，描边/凹陷的计算 MUST 与修复前一致（同一公式、同一阈值），以保证 SC-004。

## 4. ResidueCandidate / ResidueAudit（残留与审计）

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `ResidueCandidate` | `{x, y, depth, coverage}` | 满足 `depth < 1-ε ∧ coverage ≤ gate_threshold` 的像素 |
| `cluster` | `{bbox, area, centroid}` | 相邻候选像素的连通簇（8 邻域） |
| `ResidueAudit` | 见下 | 一次审计的结论 |

```text
ResidueAudit {
  frame: u64,
  pass_size: (u32, u32),
  candidates: u32,          // 候选像素总数
  clusters: u32,            // 连通簇数量
  largest_area: u32,        // 最大簇面积
  verdict: CLEAN | RESIDUE,
  hypothesis: HISTORIC_POSE | CURRENT_POSE | UNKNOWN
}
```

**判定规则**

| 条件 | verdict | hypothesis |
| --- | --- | --- |
| `candidates = 0` | CLEAN | UNKNOWN |
| `candidates > 0` 且簇的包围盒与当前姿态投影**不重叠** | RESIDUE | HISTORIC_POSE（对应研究 R1 的假设 A） |
| `candidates > 0` 且簇与当前姿态投影**重叠** | RESIDUE | CURRENT_POSE（对应假设 B） |

`hypothesis` 只用于把研究结论钉死，不参与修复决策——两种情况下主修相同（研究 R2）。

## 5. BaselineRecord（视觉基线）

用于 SC-004 的"不退化"判定。

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `pose_id` | String | 相机 + 关节组合的稳定标识（如 `default-view/zero-pose`） |
| `image` | Path | 修复前基线截图 |
| `diff_ratio` | f32 | 修复后与基线在非边缘区域的像素差异比例，上限 0.02 |

**不变式**

- **I-7**：`diff_ratio ≤ 0.02`，且差异像素必须落在抗锯齿边缘带（对应 SC-004）。

## 6. 状态迁移：一次绘制的生命周期

```text
[Idle] --draw_walk--> [Sized(pass_size)] --clear--> [Cleared] --geometry--> [Drawn] --composite--> [Idle]
```

| 迁移 | 允许的违约 | 违约后果 |
| --- | --- | --- |
| Sized → Cleared | 尺寸变了但未完整清零 | 残留（A 类） |
| Cleared → Drawn | 同一帧内二次写入深度而未写颜色 | 残留（B 类） |
| Drawn → 合成 | 合成读了与绘制不同的附件版本 | 残留（等价 A） |

**规则**：`Sized` 迁移到 `Cleared` 是**每帧无条件**的；`Drawn` 之前允许存在任意次几何绘制，但每次绘制若写深度则必须同时写颜色（除明确的深度专用通道，而本组件没有该通道）。

## 7. 与规格的映射

| 规格条目 | 数据模型落点 |
| --- | --- |
| FR-001 | FrameState 不变式 I-1/I-2 + 生命周期规则 |
| FR-002 | 生命周期：`Sized → Cleared` 在单帧内完成 |
| FR-003 | FrameState `joint_angles` 与可见内容的同帧一致性（I-2） |
| FR-004 | I-1（`pass_size` 派生自当帧视口） |
| FR-005 | Coverage I-5 + ResidueAudit `verdict = CLEAN` |
| FR-006 | 同一组件被 5 个界面实例化，规则与界面无关 |
| FR-007 | BaselineRecord + I-6 |
| FR-008 | 审计默认关闭（见 contracts/debug-hooks.md） |
| FR-009 | 数据模型不涉及模型加载/关节映射字段，改动面为零 |
| FR-010 | 门控不受 `URDF_EDGE`/`URDF_AO` 开关影响：关掉效果时无描边可残留，开效果时门控生效 |
