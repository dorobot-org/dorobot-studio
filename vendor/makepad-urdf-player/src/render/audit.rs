//! 3D 视图残留审计。
//!
//! 背景：相机或关节移动后，画面上会留下"内部为背景色、只有轮廓"的历史痕迹。
//! 该痕迹的成因是"深度存在、颜色为空"的像素被合成阶段的描边项消费。本模块把
//! 这种像素**数出来**，让验收从目测变成计数（见 specs/001-fix-ghost-outline）。
//!
//! 这里只放不依赖 GPU 的纯逻辑：阈值、判定、连通簇、报告格式。读回与绘制挂在
//! `robot_view.rs`，以便本模块可以直接被 CI 的单元测试覆盖。

/// 深度小于 `1 - DEPTH_EPS` 视为"该像素有几何"。
pub const DEPTH_EPS: f32 = 1.0e-6;

/// 清空后的深度值。
pub const FULL_DEPTH: f32 = 1.0;

/// 一次审计的结论。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Clean,
    Residue,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "CLEAN",
            Self::Residue => "RESIDUE",
        }
    }
}

/// 残留的归属假设：用于裁决"跨帧累积"与"同帧二次深度写入"。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hypothesis {
    Unknown,
    HistoricPose,
    CurrentPose,
}

impl Hypothesis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::HistoricPose => "HISTORIC_POSE",
            Self::CurrentPose => "CURRENT_POSE",
        }
    }
}

/// 审计报告。字段名与顺序是诊断契约的一部分（contracts/debug-hooks.md §2）。
#[derive(Clone, Debug, PartialEq)]
pub struct AuditReport {
    pub frame: u64,
    pub width: u32,
    pub height: u32,
    pub gate: f32,
    pub candidates: u32,
    pub clusters: u32,
    pub largest_area: u32,
    pub verdict: Verdict,
    pub hypothesis: Hypothesis,
}

impl AuditReport {
    /// 单行 JSON，供 `grep`/`jq` 直接消费。
    pub fn to_json(&self) -> String {
        format!(
            "{{\"URDF_GHOST_AUDIT\":1,\"frame\":{},\"pass_size\":[{},{}],\"gate\":{:.2},\
             \"candidates\":{},\"clusters\":{},\"largest_area\":{},\"verdict\":\"{}\",\
             \"hypothesis\":\"{}\"}}",
            self.frame,
            self.width,
            self.height,
            self.gate,
            self.candidates,
            self.clusters,
            self.largest_area,
            self.verdict.as_str(),
            self.hypothesis.as_str(),
        )
    }
}

/// 参与描边所需的最低颜色覆盖：超采样把每个输出像素拆成 `ssaa²` 个子样本，
/// 因此阈值为其倒数。SSAA 2x → 0.25（data-model.md `Coverage.gate_threshold`）。
pub fn gate_threshold(ssaa_scale: f32) -> f32 {
    if !ssaa_scale.is_finite() || ssaa_scale < 1.0 {
        return 1.0;
    }
    1.0 / (ssaa_scale * ssaa_scale)
}

/// 残留判据：有深度、没有（足够的）颜色覆盖。
pub fn is_residue(depth: f32, coverage: f32, gate: f32) -> bool {
    depth < FULL_DEPTH - DEPTH_EPS && coverage <= gate
}

/// 对一帧的深度/覆盖缓冲做一次审计。
///
/// - `current_pose`：当前姿态在屏幕上的包围盒 `[x0, y0, x1, y1]`（含端点）。
///   残留落在盒内 → `CURRENT_POSE`（同帧二次写入）；落在盒外 → `HISTORIC_POSE`（跨帧累积）。
pub fn audit(
    depth: &[f32],
    coverage: &[f32],
    width: u32,
    height: u32,
    gate: f32,
    current_pose: Option<[u32; 4]>,
    frame: u64,
) -> AuditReport {
    let len = (width as usize)
        .saturating_mul(height as usize)
        .min(depth.len())
        .min(coverage.len());
    let flags: Vec<bool> = (0..len)
        .map(|i| is_residue(depth[i], coverage[i], gate))
        .collect();
    audit_flags(&flags, width, height, gate, current_pose, frame)
}

/// 审计读回的 RGBA 掩码（见 `contracts/debug-hooks.md` §2.1）：红色通道 > 127 视为命中。
pub fn audit_mask(
    mask: &[u8],
    width: u32,
    height: u32,
    gate: f32,
    current_pose: Option<[u32; 4]>,
    frame: u64,
) -> AuditReport {
    let flags: Vec<bool> = mask.chunks_exact(4).map(|px| px[0] > 127).collect();
    audit_flags(&flags, width, height, gate, current_pose, frame)
}

/// 对已判定好的掩码做计数与聚类。
pub fn audit_flags(
    flags: &[bool],
    width: u32,
    height: u32,
    gate: f32,
    current_pose: Option<[u32; 4]>,
    frame: u64,
) -> AuditReport {
    let (w, h) = (width as usize, height as usize);
    let len = w.saturating_mul(h).min(flags.len());
    let mut candidates = 0u32;
    let mut clusters = 0u32;
    let mut largest_area = 0u32;
    let mut in_pose = false;
    let mut seen = vec![false; len];

    for start in 0..len {
        if seen[start] || !flags[start] {
            continue;
        }
        let mut area = 0u32;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(i) = stack.pop() {
            area += 1;
            let (x, y) = ((i % w) as i32, (i / w) as i32);
            if let Some([x0, y0, x1, y1]) = current_pose {
                if x >= x0 as i32 && x <= x1 as i32 && y >= y0 as i32 && y <= y1 as i32 {
                    in_pose = true;
                }
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if !seen[j] && flags[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        candidates += area;
        clusters += 1;
        largest_area = largest_area.max(area);
    }

    let verdict = if candidates == 0 {
        Verdict::Clean
    } else {
        Verdict::Residue
    };
    let hypothesis = match verdict {
        Verdict::Clean => Hypothesis::Unknown,
        Verdict::Residue if in_pose => Hypothesis::CurrentPose,
        Verdict::Residue => Hypothesis::HistoricPose,
    };

    AuditReport {
        frame,
        width,
        height,
        gate,
        candidates,
        clusters,
        largest_area,
        verdict,
        hypothesis,
    }
}

/// 合成阶段的覆盖门控：没有（足够）颜色覆盖的像素不参与描边/凹陷，`shade` 归 1.0。
///
/// 这条函数是着色器里那两行的参考实现，用它的测试把语义钉住。
pub fn shade_with_coverage(shade: f32, coverage: f32, gate: f32) -> f32 {
    if coverage <= gate {
        1.0
    } else {
        shade
    }
}

/// `URDF_GHOST_GUARD`：覆盖门控默认开启；只有显式 `0` 才关闭。
pub fn ghost_guard_enabled(raw: Option<&str>) -> bool {
    raw.is_none_or(|value| value != "0")
}

/// `URDF_GHOST_AUDIT`：审计默认关闭；只有显式 `1` 才开启。
pub fn ghost_audit_enabled(raw: Option<&str>) -> bool {
    raw.is_some_and(|value| value == "1")
}
