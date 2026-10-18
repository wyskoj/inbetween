//! The fixed scene behind `cargo xtask perf`: a jittered grid of dots on Image 1, each nudged
//! smoothly on Image 2 so the mesh never folds. Deterministic, so every run and every
//! renderer times exactly the same work.

/// Frame size of the perf scene.
pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;
/// 40 x 25 = 1,000 dot pairs.
pub const COLS: u32 = 40;
pub const ROWS: u32 = 25;

struct Rng(u64);

impl Rng {
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// `((x on Image 1, y on Image 1), (x on Image 2, y on Image 2))` for every dot.
pub fn dot_pairs() -> Vec<((f64, f64), (f64, f64))> {
    let (cw, ch) = (
        f64::from(WIDTH) / f64::from(COLS),
        f64::from(HEIGHT) / f64::from(ROWS),
    );
    let mut rng = Rng(0x5EED_1234_ABCD_9876);
    let mut pairs = Vec::new();
    for row in 0..ROWS {
        for col in 0..COLS {
            let x = (f64::from(col) + 0.5 + (rng.unit() - 0.5) * 0.5) * cw;
            let y = (f64::from(row) + 0.5 + (rng.unit() - 0.5) * 0.5) * ch;
            let dx = (y / 140.0).sin() * 0.3 * cw;
            let dy = (x / 170.0).cos() * 0.3 * ch;
            pairs.push(((x, y), (x + dx, y + dy)));
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_has_a_thousand_distinct_dots_near_the_frame() {
        let pairs = dot_pairs();
        assert_eq!(pairs.len(), 1000);
        for (i, ((x, y), (x2, y2))) in pairs.iter().enumerate() {
            assert!(
                *x > 0.0 && *x < f64::from(WIDTH) && *y > 0.0 && *y < f64::from(HEIGHT),
                "{i}"
            );
            assert!(
                // Image 2 dots may drift a little outside the frame, which is allowed.
                *x2 > -50.0
                    && *x2 < f64::from(WIDTH) + 50.0
                    && *y2 > -50.0
                    && *y2 < f64::from(HEIGHT) + 50.0,
                "{i}"
            );
            assert!(pairs[i + 1..].iter().all(|((a, b), _)| (a, b) != (x, y)));
        }
    }

    #[test]
    fn scene_is_deterministic() {
        assert_eq!(dot_pairs(), dot_pairs());
    }
}
