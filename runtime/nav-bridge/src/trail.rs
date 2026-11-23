//! Breadcrumb trail of robot positions.

pub struct Trail {
    min_step: f32,
    max_points: usize,
    points: Vec<[f32; 2]>,
    goal_id: Option<String>,
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

    /// A goal id different from the previous one clears the trail. The first id
    /// seen only records itself.
    pub fn on_goal(&mut self, goal_id: &str) {
        match &self.goal_id {
            Some(prev) if prev == goal_id => {}
            Some(_) => {
                self.points.clear();
                self.goal_id = Some(goal_id.to_owned());
            }
            None => self.goal_id = Some(goal_id.to_owned()),
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
        assert_eq!(t.points().len(), 2, "first goal keeps the existing trail");
        t.on_goal("C1");
        assert_eq!(t.points().len(), 2);
        t.on_goal("C2");
        assert!(t.points().is_empty());
        t.push([1.0, 0.0]);
        assert_eq!(t.points().len(), 1);
    }

    #[test]
    fn trail_ignores_non_finite() {
        let mut t = Trail::new(0.05, 10);
        t.push([f64::NAN, 0.0]);
        assert!(t.points().is_empty());
    }
}
