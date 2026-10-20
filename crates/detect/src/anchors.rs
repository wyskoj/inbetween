//! SSD anchors and box decoding for the 128x128 short-range face detector, plus weighted NMS.

/// Network input side, in pixels.
pub const INPUT: usize = 128;
/// Anchors for the short-range model: 16x16x2 + 8x8x6.
pub const ANCHOR_COUNT: usize = 896;
/// Raw values per anchor: box (4) + 6 keypoints (12).
pub const REGRESSORS: usize = 16;

/// An anchor centre in 0..1 (width and height are fixed at 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub x: f32,
    pub y: f32,
}

/// The fixed anchor grid: strides 8, 16, 16, 16 with 2, 2, 2, 2 anchors per cell; the three
/// stride-16 layers share one feature map, giving 6 anchors per cell.
pub fn generate() -> Vec<Anchor> {
    let mut out = Vec::with_capacity(ANCHOR_COUNT);
    for (stride, per_cell) in [(8usize, 2usize), (16, 6)] {
        let cells = INPUT / stride;
        for y in 0..cells {
            for x in 0..cells {
                for _ in 0..per_cell {
                    out.push(Anchor {
                        x: (x as f32 + 0.5) / cells as f32,
                        y: (y as f32 + 0.5) / cells as f32,
                    });
                }
            }
        }
    }
    out
}

/// A decoded detection in letterboxed-input coordinates, 0..1.
#[derive(Clone, Debug, PartialEq)]
pub struct Detection {
    pub score: f32,
    /// Box centre and size.
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
    /// Right eye, left eye, nose, mouth, right ear, left ear (image order as the model emits).
    pub keypoints: [(f32, f32); 6],
}

fn sigmoid(v: f32) -> f32 {
    1.0 / (1.0 + (-v).exp())
}

/// Decode raw regressors and scores into detections above `min_score`. Slices of the wrong
/// length give an empty result.
pub fn decode(raw: &[f32], scores: &[f32], anchors: &[Anchor], min_score: f32) -> Vec<Detection> {
    let n = anchors.len();
    if raw.len() < n * REGRESSORS || scores.len() < n {
        return Vec::new();
    }
    let scale = INPUT as f32;
    let mut out = Vec::new();
    for (i, a) in anchors.iter().enumerate() {
        let score = sigmoid(
            scores
                .get(i)
                .copied()
                .unwrap_or(f32::MIN)
                .clamp(-80.0, 80.0),
        );
        if score.is_nan() || score < min_score {
            continue;
        }
        let r = match raw.get(i * REGRESSORS..(i + 1) * REGRESSORS) {
            Some(r) => r,
            None => continue,
        };
        let cx = r[0] / scale + a.x;
        let cy = r[1] / scale + a.y;
        let (w, h) = (r[2] / scale, r[3] / scale);
        let mut keypoints = [(0.0, 0.0); 6];
        for (k, kp) in keypoints.iter_mut().enumerate() {
            *kp = (r[4 + 2 * k] / scale + a.x, r[5 + 2 * k] / scale + a.y);
        }
        let d = Detection {
            score,
            cx,
            cy,
            w,
            h,
            keypoints,
        };
        if [cx, cy, w, h].iter().all(|v| v.is_finite()) && w > 0.0 && h > 0.0 {
            out.push(d);
        }
    }
    out
}

fn iou(a: &Detection, b: &Detection) -> f32 {
    let (ax0, ay0, ax1, ay1) = (
        a.cx - a.w / 2.0,
        a.cy - a.h / 2.0,
        a.cx + a.w / 2.0,
        a.cy + a.h / 2.0,
    );
    let (bx0, by0, bx1, by1) = (
        b.cx - b.w / 2.0,
        b.cy - b.h / 2.0,
        b.cx + b.w / 2.0,
        b.cy + b.h / 2.0,
    );
    let iw = (ax1.min(bx1) - ax0.max(bx0)).max(0.0);
    let ih = (ay1.min(by1) - ay0.max(by0)).max(0.0);
    let inter = iw * ih;
    let union = a.w * a.h + b.w * b.h - inter;
    if union > 0.0 {
        inter / union
    } else {
        0.0
    }
}

/// Weighted non-maximum suppression: overlapping detections (IoU above `threshold`) merge
/// into one whose geometry is the score-weighted mean. Output is sorted by score, ties
/// broken left to right, so it is deterministic.
pub fn weighted_nms(mut dets: Vec<Detection>, threshold: f32) -> Vec<Detection> {
    dets.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.cx.total_cmp(&b.cx))
            .then(a.cy.total_cmp(&b.cy))
    });
    let mut out = Vec::new();
    while !dets.is_empty() {
        let best = dets.remove(0);
        let (group, rest): (Vec<_>, Vec<_>) =
            dets.into_iter().partition(|d| iou(&best, d) > threshold);
        dets = rest;
        let mut total = best.score;
        let mut m = best.clone();
        let wmul = |v: f32, s: f32| v * s;
        m.cx = wmul(best.cx, best.score);
        m.cy = wmul(best.cy, best.score);
        m.w = wmul(best.w, best.score);
        m.h = wmul(best.h, best.score);
        for (k, kp) in m.keypoints.iter_mut().enumerate() {
            *kp = (
                best.keypoints[k].0 * best.score,
                best.keypoints[k].1 * best.score,
            );
        }
        for g in &group {
            total += g.score;
            m.cx += g.cx * g.score;
            m.cy += g.cy * g.score;
            m.w += g.w * g.score;
            m.h += g.h * g.score;
            for (k, kp) in m.keypoints.iter_mut().enumerate() {
                kp.0 += g.keypoints[k].0 * g.score;
                kp.1 += g.keypoints[k].1 * g.score;
            }
        }
        if total > 0.0 {
            m.cx /= total;
            m.cy /= total;
            m.w /= total;
            m.h /= total;
            for kp in &mut m.keypoints {
                kp.0 /= total;
                kp.1 /= total;
            }
        }
        out.push(m);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det(score: f32, cx: f32) -> Detection {
        Detection {
            score,
            cx,
            cy: 0.5,
            w: 0.2,
            h: 0.2,
            keypoints: [(cx, 0.5); 6],
        }
    }

    #[test]
    fn anchor_grid_has_the_fixed_size() {
        assert_eq!(generate().len(), ANCHOR_COUNT);
    }

    #[test]
    fn wrong_length_tensors_decode_to_nothing() {
        assert!(decode(&[0.0; 10], &[0.0; 10], &generate(), 0.5).is_empty());
    }

    #[test]
    fn nan_scores_are_dropped() {
        let a = generate();
        let raw = vec![0.0; ANCHOR_COUNT * REGRESSORS];
        let scores = vec![f32::NAN; ANCHOR_COUNT];
        assert!(decode(&raw, &scores, &a, 0.5).is_empty());
    }

    #[test]
    fn nms_merges_overlaps_and_keeps_separate_faces() {
        let out = weighted_nms(vec![det(0.9, 0.30), det(0.8, 0.31), det(0.7, 0.80)], 0.3);
        assert_eq!(out.len(), 2);
        assert!(out[0].cx < out[1].cx || out[0].score >= out[1].score);
        assert!((out[0].cx - 0.30).abs() < 0.02);
    }
}
