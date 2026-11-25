//! T003（先红）：残留判定与审计输出的契约测试。
//! 对应 specs/001-fix-ghost-outline/{data-model.md,contracts/visual-invariant.md}。
use makepad_urdf_player::render::audit::{
    audit, gate_threshold, ghost_audit_enabled, ghost_guard_enabled, is_residue, shade_with_coverage,
    Hypothesis, Verdict,
};

#[test]
fn gate_is_one_over_supersample_squared() {
    // data-model.md Coverage.gate_threshold：SSAA 2x → 0.25
    assert_eq!(gate_threshold(2.0), 0.25);
    assert_eq!(gate_threshold(1.0), 1.0);
    // 非法倍率不得产生 NaN 或 0
    assert_eq!(gate_threshold(0.0), 1.0);
}

#[test]
fn residue_needs_depth_without_coverage() {
    let gate = gate_threshold(2.0);
    assert!(is_residue(0.5, 0.0, gate), "有深度无覆盖 = 残留");
    assert!(!is_residue(1.0, 0.0, gate), "清空深度不是残留");
    assert!(!is_residue(0.5, 0.9, gate), "有覆盖不是残留");
    assert!(is_residue(0.5, gate, gate), "门限取等号仍算残留（inline 边缘不带描边）");
    assert!(!is_residue(0.5, gate + 0.01, gate));
}

#[test]
fn audit_counts_clusters_and_largest_area() {
    // 5x5：左上三个连成一片，右下单独一个；其余为空
    let (w, h) = (5u32, 5u32);
    let mut depth = vec![1.0f32; (w * h) as usize];
    let coverage = vec![0.0f32; (w * h) as usize];
    for (x, y) in [(0, 0), (1, 0), (0, 1), (4, 3)] {
        depth[(y * w + x) as usize] = 0.4;
    }
    let report = audit(&depth, &coverage, w, h, gate_threshold(2.0), None, 7);
    assert_eq!(report.candidates, 4);
    assert_eq!(report.clusters, 2);
    assert_eq!(report.largest_area, 3);
    assert_eq!(report.verdict, Verdict::Residue);
    assert_eq!(report.hypothesis, Hypothesis::HistoricPose);
    assert_eq!(report.frame, 7);
}

#[test]
fn clean_frame_reports_clean_and_unknown() {
    let (w, h) = (3u32, 3u32);
    let depth = vec![1.0f32; 9];
    let coverage = vec![0.0f32; 9];
    let report = audit(&depth, &coverage, w, h, gate_threshold(2.0), None, 1);
    assert_eq!(report.candidates, 0);
    assert_eq!(report.clusters, 0);
    assert_eq!(report.verdict, Verdict::Clean);
    assert_eq!(report.hypothesis, Hypothesis::Unknown);
}

#[test]
fn residue_inside_current_pose_is_reported_as_current_pose() {
    let (w, h) = (4u32, 4u32);
    let mut depth = vec![1.0f32; 16];
    depth[5] = 0.3; // (1,1)
    let coverage = vec![0.0f32; 16];
    let report = audit(&depth, &coverage, w, h, gate_threshold(2.0), Some([0, 0, 2, 2]), 3);
    assert_eq!(report.verdict, Verdict::Residue);
    assert_eq!(report.hypothesis, Hypothesis::CurrentPose);
}

#[test]
fn report_json_matches_the_debug_hook_contract() {
    let report = audit(&vec![1.0f32; 4], &vec![0.0f32; 4], 2, 2, 0.25, None, 42);
    let json = report.to_json();
    for needle in [
        "\"frame\":42",
        "\"pass_size\":[2,2]",
        "\"gate\":0.25",
        "\"candidates\":0",
        "\"verdict\":\"CLEAN\"",
        "\"hypothesis\":\"UNKNOWN\"",
    ] {
        assert!(json.contains(needle), "缺少 {needle}：{json}");
    }
}

#[test]
fn env_switches_default_off_and_off_is_explicit() {
    // contracts/debug-hooks.md：门控默认开启，审计默认关闭
    assert!(ghost_guard_enabled(None));
    assert!(!ghost_guard_enabled(Some("0")));
    assert!(ghost_guard_enabled(Some("1")));

    assert!(!ghost_audit_enabled(None));
    assert!(ghost_audit_enabled(Some("1")));
    assert!(!ghost_audit_enabled(Some("0")));
}

#[test]
fn uncovered_pixels_are_never_shaded() {
    // contracts/visual-invariant.md §1：覆盖不足时不得压暗；覆盖足够时公式不变。
    let gate = gate_threshold(2.0);
    assert_eq!(shade_with_coverage(0.42, 0.0, gate), 1.0);
    assert_eq!(shade_with_coverage(0.42, gate, gate), 1.0);
    assert_eq!(shade_with_coverage(0.42, 0.75, gate), 0.42);
}
