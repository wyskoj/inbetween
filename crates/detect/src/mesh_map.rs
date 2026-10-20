//! Inbetween's own table from standard-model landmark names to indexes of a 468-point face
//! mesh (the canonical topology of the bundled landmark network). A landmark with several
//! indexes sits at their mean. Names the table does not know are placed by a least-squares
//! similarity fit from the known ones, so custom models still work.

use inbetween_geom::Point;

/// Number of mesh points the table indexes into.
pub const MESH_POINTS: usize = 468;

/// Mesh indexes for a standard-model landmark name. "Right" is the subject's right, which
/// appears on the left of the image.
pub fn indices(name: &str) -> Option<&'static [usize]> {
    Some(match name {
        "jaw_right_top" => &[234],
        "jaw_right_mid" => &[132],
        "jaw_right_low" => &[172],
        "jaw_right_chin" => &[148],
        "chin" => &[152],
        "jaw_left_chin" => &[377],
        "jaw_left_low" => &[397],
        "jaw_left_mid" => &[361],
        "jaw_left_top" => &[454],
        // The mesh stops at the forehead, so these are approximate.
        "hairline_right" => &[103],
        "hairline_mid" => &[10],
        "hairline_left" => &[332],
        "brow_right_outer" => &[70],
        "brow_right_peak" => &[105],
        "brow_right_inner" => &[107],
        "brow_left_inner" => &[336],
        "brow_left_peak" => &[334],
        "brow_left_outer" => &[300],
        "eye_right_outer" => &[33],
        "eye_right_top" => &[159],
        "eye_right_inner" => &[133],
        "eye_right_bottom" => &[145],
        "pupil_right" => &[33, 133, 159, 145],
        "eye_left_inner" => &[362],
        "eye_left_top" => &[386],
        "eye_left_outer" => &[263],
        "eye_left_bottom" => &[374],
        "pupil_left" => &[362, 263, 386, 374],
        "nose_bridge" => &[168],
        "nose_tip" => &[1],
        "nostril_right" => &[129],
        "nostril_left" => &[358],
        "mouth_right_corner" => &[61],
        "lip_top_right" => &[37],
        "lip_top_mid" => &[0],
        "lip_top_left" => &[267],
        "mouth_left_corner" => &[291],
        "lip_bottom_mid" => &[17],
        _ => return None,
    })
}

/// Position of a named landmark on `mesh`, or `None` for an unknown name or short mesh.
pub fn point(name: &str, mesh: &[Point]) -> Option<Point> {
    let idx = indices(name)?;
    let mut sum = Point::new(0.0, 0.0);
    for &i in idx {
        sum = sum + *mesh.get(i)?;
    }
    let p = sum * (1.0 / idx.len() as f64);
    p.is_finite().then_some(p)
}

/// Least-squares similarity transform (scale, rotation, translation) taking `from` onto `to`.
/// Returns `[a, b, tx, ty]` with `x' = a x - b y + tx`, `y' = b x + a y + ty`.
fn similarity(from: &[Point], to: &[Point]) -> Option<[f64; 4]> {
    let n = from.len().min(to.len());
    if n < 2 {
        return None;
    }
    let nf = n as f64;
    let (mut fx, mut fy, mut tx, mut ty) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        fx += from[i].x;
        fy += from[i].y;
        tx += to[i].x;
        ty += to[i].y;
    }
    let (fx, fy, tx, ty) = (fx / nf, fy / nf, tx / nf, ty / nf);
    let (mut num_a, mut num_b, mut den) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (px, py) = (from[i].x - fx, from[i].y - fy);
        let (qx, qy) = (to[i].x - tx, to[i].y - ty);
        num_a += px * qx + py * qy;
        num_b += px * qy - py * qx;
        den += px * px + py * py;
    }
    if den < 1e-12 {
        return None;
    }
    let (a, b) = (num_a / den, num_b / den);
    let out = [a, b, tx - (a * fx - b * fy), ty - (b * fx + a * fy)];
    out.iter().all(|v| v.is_finite()).then_some(out)
}

/// Place every model landmark on `mesh`. `landmarks` gives `(name, model_x, model_y)` for each
/// landmark to place, in order. Known names come from the mesh; unknown names are placed by
/// a similarity fit from the known ones (model space to image). `None` when the mesh is too
/// short, or when unknown names exist but fewer than two are known.
pub fn place(landmarks: &[(&str, f64, f64)], mesh: &[Point]) -> Option<Vec<Point>> {
    let known: Vec<Option<Point>> = landmarks.iter().map(|(n, _, _)| point(n, mesh)).collect();
    let (mut from, mut to) = (Vec::new(), Vec::new());
    for (l, k) in landmarks.iter().zip(&known) {
        if let Some(p) = k {
            from.push(Point::new(l.1, l.2));
            to.push(*p);
        }
    }
    let fit = if known.iter().any(Option::is_none) {
        Some(similarity(&from, &to)?)
    } else {
        None
    };
    let mut out = Vec::with_capacity(landmarks.len());
    for (l, k) in landmarks.iter().zip(&known) {
        let p = match (k, fit) {
            (Some(p), _) => *p,
            (None, Some([a, b, tx, ty])) => {
                Point::new(a * l.1 - b * l.2 + tx, b * l.1 + a * l.2 + ty)
            }
            (None, None) => return None,
        };
        if !p.is_finite() {
            return None;
        }
        out.push(p);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "jaw_right_top",
        "jaw_right_mid",
        "jaw_right_low",
        "jaw_right_chin",
        "chin",
        "jaw_left_chin",
        "jaw_left_low",
        "jaw_left_mid",
        "jaw_left_top",
        "hairline_right",
        "hairline_mid",
        "hairline_left",
        "brow_right_outer",
        "brow_right_peak",
        "brow_right_inner",
        "brow_left_inner",
        "brow_left_peak",
        "brow_left_outer",
        "eye_right_outer",
        "eye_right_top",
        "eye_right_inner",
        "eye_right_bottom",
        "pupil_right",
        "eye_left_inner",
        "eye_left_top",
        "eye_left_outer",
        "eye_left_bottom",
        "pupil_left",
        "nose_bridge",
        "nose_tip",
        "nostril_right",
        "nostril_left",
        "mouth_right_corner",
        "lip_top_right",
        "lip_top_mid",
        "lip_top_left",
        "mouth_left_corner",
        "lip_bottom_mid",
    ];

    #[test]
    fn every_standard_name_maps_below_the_mesh_size() {
        assert_eq!(NAMES.len(), 38);
        for n in NAMES {
            let idx = indices(n).unwrap_or_default();
            assert!(!idx.is_empty(), "{n}");
            assert!(idx.iter().all(|&i| i < MESH_POINTS), "{n}");
        }
    }

    fn mesh() -> Vec<Point> {
        (0..MESH_POINTS)
            .map(|i| Point::new((i % 30) as f64 * 3.0, (i / 30) as f64 * 3.0))
            .collect()
    }

    #[test]
    fn unknown_names_follow_a_similarity_fit() {
        // Model space is the unit square; mesh points sit at 2x scale, shifted by (10, 20).
        let m = mesh();
        let known = point("nose_tip", &m).expect("known");
        let other = point("chin", &m).expect("known");
        let lm = [
            ("nose_tip", 0.0, 0.0),
            ("chin", 1.0, 0.0),
            ("mystery", 0.5, 0.0),
        ];
        let out = place(&lm, &m).expect("placed");
        assert_eq!(out[0], known);
        let mid = known.lerp(other, 0.5);
        assert!(out[2].distance(mid) < 1e-6);
    }

    #[test]
    fn short_meshes_and_too_few_anchors_fail_cleanly() {
        assert!(place(&[("nose_tip", 0.5, 0.5)], &[Point::new(0.0, 0.0)]).is_none());
        let m = mesh();
        assert!(place(&[("mystery", 0.5, 0.5)], &m).is_none());
    }
}
