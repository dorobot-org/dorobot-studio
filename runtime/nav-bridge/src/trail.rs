//! Breadcrumb trail of robot positions.

pub struct Trail {
    min_step: f32,
    max_points: usize,
    points: Vec<[f32; 2]>,
    goal_id: Option<String>,
}

/// The goal id without goal_resume's resend suffix: `C1-r2` is `C1` (DORA_NAV
/// goal_resume resends a goal as `<id>-r<n>` after a localization loss).
pub fn base_goal_id(id: &str) -> &str {
    match id.rsplit_once("-r") {
        Some((base, n)) if !base.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => base,
        _ => id,
    }
}

impl Trail {
    pub fn new(min_step: f32, max_points: usize) -> Self {
        Trail { min_step, max_points, points: Vec::new(), goal_id: None }
    }

    /// Ignored when closer than `min_step` to the last point (or non-finite);
    /// the oldest point is dropped above `max_points`.
    pub fn push(&mut self, p: [f64; 2]) {
        if !p[0].is_finite() || !p[1].is_finite() || self.max_points == 0 {
            return;
        }
        let p = [p[0] as f32, p[1] as f32];
        if let Some(last) = self.points.last() {
            if (p[0] - last[0]).hypot(p[1] - last[1]) < self.min_step {
                return;
            }
        }
        self.points.push(p);
        if self.points.len() > self.max_points {
            let extra = self.points.len() - self.max_points;
            self.points.drain(..extra);
        }
    }

    /// A goal id different from the previous one clears the trail; going from no
    /// goal to the first id counts as a change. Ids are compared without the resend
    /// suffix ([`base_goal_id`]), so a resumed goal keeps its trail.
    pub fn on_goal(&mut self, goal_id: &str) {
        let base = base_goal_id(goal_id);
        if self.goal_id.as_deref() != Some(base) {
            self.points.clear();
            self.goal_id = Some(base.to_owned());
        }
    }

    pub fn points(&self) -> &[[f32; 2]] {
        &self.points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trail_spacing_and_cap() {
        let mut t = Trail::new(0.05, 3000);
        for i in 0..100 {
            t.push([i as f64 * 0.01, 0.0]);
        }
        let n = t.points().len();
        assert!((19..=21).contains(&n), "{n}");
        let mut t = Trail::new(0.05, 3);
        for i in 0..10 {
            t.push([i as f64, 0.0]);
        }
        assert_eq!(t.points(), &[[7.0, 0.0], [8.0, 0.0], [9.0, 0.0]]);
    }

    #[test]
    fn trail_resets_on_new_goal_id() {
        let mut t = Trail::new(0.05, 100);
        t.push([0.0, 0.0]);
        t.push([1.0, 0.0]);
        t.on_goal("C1");
        assert!(t.points().is_empty(), "first goal id clears points from before it");
        t.push([0.0, 0.0]);
        t.push([1.0, 0.0]);
        t.on_goal("C1");
        assert_eq!(t.points().len(), 2, "same id keeps the trail");
        t.on_goal("C2");
        assert!(t.points().is_empty());
        t.push([1.0, 0.0]);
        assert_eq!(t.points().len(), 1);
    }

    /// goal_resume resends the goal as `<id>-r<n>` after a localization loss
    /// (DORA_NAV resume_node_core.cpp): the same goal, so the trail is kept.
    #[test]
    fn a_resent_goal_keeps_the_trail() {
        let mut t = Trail::new(0.05, 100);
        t.on_goal("C1");
        t.push([0.0, 0.0]);
        t.push([1.0, 0.0]);
        for id in ["C1-r1", "C1-r2", "C1", "C1-r12"] {
            t.on_goal(id);
            assert_eq!(t.points().len(), 2, "{id} is C1 resent");
        }
        t.on_goal("C2-r1");
        assert!(t.points().is_empty(), "another goal, resent or not");
        t.push([1.0, 0.0]);
        for id in ["C2", "C2-r", "C2-rx"] {
            t.on_goal(id);
        }
        assert!(t.points().is_empty(), "C2-r and C2-rx are other ids");
    }

    #[test]
    fn base_goal_id_strips_one_resend_suffix() {
        assert_eq!(base_goal_id("C1-r3"), "C1");
        assert_eq!(base_goal_id("C1"), "C1");
        assert_eq!(base_goal_id("C1-r"), "C1-r");
        assert_eq!(base_goal_id("C1-r3x"), "C1-r3x");
        assert_eq!(base_goal_id("-r3"), "-r3");
        assert_eq!(base_goal_id("goal-r1-r2"), "goal-r1");
    }

    #[test]
    fn trail_ignores_non_finite() {
        let mut t = Trail::new(0.05, 10);
        t.push([f64::NAN, 0.0]);
        assert!(t.points().is_empty());
    }
}
