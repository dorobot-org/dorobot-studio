# Quickstart：验收这次修复

**Feature**: 001-fix-ghost-outline

本文件只给"怎么验"，不含实现步骤。实现细节在 `tasks.md`。

## 0. 前置

- macOS 桌面端，已构建过 `dorobot-flex`（首次构建见 `docs/ONBOARDING.md` Day 0）。
- 无视频构建即可：`--no-default-features`。
- 需要能看到窗口（本次自动化截图受系统屏幕录制权限限制，验收以人工观察 + 审计输出为准）。

## 1. 复现（修复前必须先看到问题）

```sh
cargo run --locked -p dorobot-flex --no-default-features -- --demo
```

1. 进入任一含 3D 机器人视图的界面（机器人设置 → 模拟；或 数据工作区 → 回放）。
2. 拖动视角 10 次，再把某个关节来回移动 10 次。
3. 停止操作，观察机器人周围的空白背景。

**预期（修复前）**：空白背景处可见 1–2 px 的细线轮廓，内部与背景同色，形状与当前姿态不符（即用户截图中的红框区域）。

若无法复现，先不要继续：说明环境或步骤与报告不一致，需回到复现条件定位。

## 2. 审计基线（把"看得见"变成"数得清"）

```sh
URDF_GHOST_AUDIT=1 cargo run --locked -p dorobot-flex --no-default-features -- --demo
```

重复 §1 的操作，从日志中取最后一条 `URDF_GHOST_AUDIT` 行：

```sh
... | grep URDF_GHOST_AUDIT | tail -1
```

**修复前的预期**：`"verdict":"RESIDUE"`，`candidates > 0`。

**同时记录 `hypothesis`**：
- `HISTORIC_POSE` → 走 `contracts/render-lifecycle.md` §4 的"跨帧累积"分支
- `CURRENT_POSE` → 走"同帧二次深度写入"分支

## 3. 修复后的验收（三条都要过）

```sh
# 3.1 审计必须归零
URDF_GHOST_AUDIT=1 cargo run --locked -p dorobot-flex --no-default-features -- --demo
# 预期：{"verdict":"CLEAN","candidates":0}

# 3.2 效果开关不能掩盖问题（两种极端组合都必须是 CLEAN）
URDF_GHOST_AUDIT=1 URDF_EDGE=0 cargo run --locked -p dorobot-flex --no-default-features -- --demo
URDF_GHOST_AUDIT=1 URDF_AO=0   cargo run --locked -p dorobot-flex --no-default-features -- --demo

# 3.3 关掉门控应能复现旧行为（证明开关有效、且修复确实来自门控）
URDF_GHOST_AUDIT=1 URDF_GHOST_GUARD=0 cargo run --locked -p dorobot-flex --no-default-features -- --demo
# 预期：回到 RESIDUE
```

## 4. 五界面人工清单（SC-003）

每个界面执行 **视角 / 关节 / 视口尺寸** 三类操作，各 3–5 次，停下后看空白背景：

| # | 界面 | 视角 | 关节 | 尺寸 |
| --- | --- | --- | --- | --- |
| 1 | 机器人设置（模拟） | ☐ | ☐ | ☐ |
| 2 | 操作（Operate） | ☐ | ☐ | ☐ |
| 3 | 实时镜像（Live mirror） | ☐ | ☐ | ☐ |
| 4 | 回放（含独立回放窗口） | ☐ | ☐ | ☐ |
| 5 | 评估（Eval） | ☐ | ☐ | ☐ |

**通过判据**：任一格内不得出现"内部为背景色、只有轮廓"的痕迹。若出现实心色块，属另一类问题，单独记录。

> 注：第 2、3、5 项在 `--demo` 下可能只显示模拟数据；只要 3D 视图可见即满足验证条件。

## 5. 视觉不退化（SC-004）

1. 修复前，在固定相机角度与零位姿态截一张图（人工截图），存为 `docs/ux/baseline-ghost-fix.png`。
2. 修复后，同一角度同一姿态再截一张。
3. 比较：轮廓与接触凹陷必须仍在；差异只允许出现在边缘带。

**判据**：非边缘区域差异 ≤ 2%。

## 6. 性能（SC-005）

```sh
# 审计关闭状态，连续拖动视角 10 秒，记录帧时间分布
```

对比修复前后的平均值与 p95：**≤ 基线的 105%**。

## 7. 回归

```sh
cargo test --locked --workspace --no-default-features
```

预期：全绿（含新增的残留判定纯逻辑单测）。CI 与本地命令一致。

## 8. 出问题时的回滚

| 现象 | 首个动作 |
| --- | --- |
| 描边变细/断裂 | 设 `URDF_GHOST_GUARD=0` 对照；确认是门控还是基线本身 |
| 审计一直 RESIDUE | 看 `hypothesis`，转 `contracts/render-lifecycle.md` §4 对应分支 |
| 帧率下降 | 确认 `URDF_GHOST_AUDIT` 未开启；未开启仍下降则回滚门控并记录 |
