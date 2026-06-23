//! Reconstruct 2-D / rank-3 **image signals** from the flat scope signal names a
//! mflowLink run produces.
//!
//! The simulator logs an image block's pixels as one scalar trace per element,
//! named `base[i,j]` for a 2-D image or `base[i,j,k]` for a rank-3 colour image
//! (`k` = channel), with 1-based MATLAB-style subscripts — the same names appear
//! in the one-shot CSV header and in the live `signalSample` stream. Grouping
//! those names back by base recovers the image's shape, so the IDE can draw it as
//! a heatmap instead of N unreadable pixel traces. Pure + GTK-free.

/// A 2-D (`channels == 1`) or rank-3 colour (`channels == 3`) image recovered
/// from a set of `base[i,j(,k)]` scope signal names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageSignal {
    pub base: String,
    pub rows: usize,
    pub cols: usize,
    pub channels: usize,
}

impl ImageSignal {
    /// The scope signal name for the `(row, col, channel)` element (all 0-based).
    pub fn element_name(&self, row: usize, col: usize, channel: usize) -> String {
        if self.channels > 1 {
            format!("{}[{},{},{}]", self.base, row + 1, col + 1, channel + 1)
        } else {
            format!("{}[{},{}]", self.base, row + 1, col + 1)
        }
    }

    /// Pixel count (`rows * cols * channels`).
    pub fn len(&self) -> usize {
        self.rows * self.cols * self.channels
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Parse `base[i,j]` / `base[i,j,k]` into its base name and 1-based indices.
/// Returns `None` for non-subscripted names, 1-D vectors (`v[3]`), or rank > 3.
fn parse_subscript(name: &str) -> Option<(&str, Vec<usize>)> {
    let open = name.rfind('[')?;
    let inner = name.strip_suffix(']')?.get(open + 1..)?;
    let base = &name[..open];
    if base.is_empty() {
        return None;
    }
    let idx: Vec<usize> = inner
        .split(',')
        .map(|s| s.trim().parse::<usize>().ok().filter(|&n| n >= 1))
        .collect::<Option<_>>()?;
    if (2..=3).contains(&idx.len()) {
        Some((base, idx))
    } else {
        None
    }
}

/// Group flat pixel-signal names into the 2-D / rank-3 images they encode. Only
/// **complete** grids (every `rows×cols×channels` element present, ≥ 2 pixels)
/// are returned, so 1-D vectors and partial/non-image signal sets are ignored.
pub fn detect_image_signals(names: &[String]) -> Vec<ImageSignal> {
    use std::collections::BTreeMap;
    struct Acc {
        dims: usize,
        max: Vec<usize>,
        count: usize,
    }
    let mut groups: BTreeMap<String, Acc> = BTreeMap::new();
    for name in names {
        let Some((base, idx)) = parse_subscript(name) else {
            continue;
        };
        let acc = groups.entry(base.to_string()).or_insert_with(|| Acc {
            dims: idx.len(),
            max: vec![0; idx.len()],
            count: 0,
        });
        if acc.dims != idx.len() {
            continue; // mixed rank under one base — not a clean image
        }
        for (m, &i) in acc.max.iter_mut().zip(&idx) {
            *m = (*m).max(i);
        }
        acc.count += 1;
    }

    groups
        .into_iter()
        .filter_map(|(base, a)| {
            let rows = a.max[0];
            let cols = a.max[1];
            let channels = if a.dims == 3 { a.max[2] } else { 1 };
            let total = rows * cols * channels;
            // Complete grid + a genuine 2-D shape (guards against a stray pair).
            if total >= 2 && a.count == total {
                Some(ImageSignal {
                    base,
                    rows,
                    cols,
                    channels,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Min/max of the finite values, for mapping pixels onto a 0..1 display range.
pub fn value_range(values: &[f64]) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &v in values.iter().filter(|v| v.is_finite()) {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if lo <= hi {
        Some((lo, hi))
    } else {
        None
    }
}

/// Normalize a pixel value into `0.0..=1.0` given a `(min, max)` range. A
/// degenerate range (min == max) maps everything to mid-grey.
pub fn normalize(v: f64, range: (f64, f64)) -> f64 {
    let (lo, hi) = range;
    if hi > lo {
        ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
    } else {
        0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn detects_a_2d_image_from_subscripted_names() {
        // A 3×3 box-filtered image plus a 2×2 thresholded one (image_blocks.mflow).
        let mut n = vec![];
        for i in 1..=3 {
            for j in 1..=3 {
                n.push(format!("sBox[{i},{j}]"));
            }
        }
        for i in 1..=2 {
            for j in 1..=2 {
                n.push(format!("sThr[{i},{j}]"));
            }
        }
        let imgs = detect_image_signals(&n);
        assert_eq!(imgs.len(), 2);
        let box_img = imgs.iter().find(|i| i.base == "sBox").unwrap();
        assert_eq!((box_img.rows, box_img.cols, box_img.channels), (3, 3, 1));
        let thr = imgs.iter().find(|i| i.base == "sThr").unwrap();
        assert_eq!((thr.rows, thr.cols, thr.channels), (2, 2, 1));
        assert_eq!(box_img.element_name(0, 0, 0), "sBox[1,1]");
        assert_eq!(box_img.element_name(2, 1, 0), "sBox[3,2]");
    }

    #[test]
    fn detects_a_rank3_colour_image() {
        // 2×2×3 RGB image (nd_color_image.mflow).
        let mut n = vec![];
        for i in 1..=2 {
            for j in 1..=2 {
                for k in 1..=3 {
                    n.push(format!("s[{i},{j},{k}]"));
                }
            }
        }
        let imgs = detect_image_signals(&n);
        assert_eq!(imgs.len(), 1);
        assert_eq!((imgs[0].rows, imgs[0].cols, imgs[0].channels), (2, 2, 3));
        assert_eq!(imgs[0].element_name(0, 1, 2), "s[1,2,3]");
        assert_eq!(imgs[0].len(), 12);
    }

    #[test]
    fn ignores_scalars_vectors_and_partial_grids() {
        // Scalars + a 1-D vector are not images.
        assert!(
            detect_image_signals(&names(&["scope", "step", "v[1]", "v[2]", "v[3]"])).is_empty()
        );
        // A ragged / incomplete 2-D grid (missing [2,2]) is rejected.
        assert!(
            detect_image_signals(&names(&["p[1,1]", "p[1,2]", "p[2,1]"])).is_empty(),
            "incomplete grid should not be treated as an image"
        );
    }

    #[test]
    fn normalizes_pixels_onto_unit_range() {
        let vals = [0.0, 5.0, 10.0];
        let r = value_range(&vals).unwrap();
        assert_eq!(r, (0.0, 10.0));
        assert_eq!(normalize(0.0, r), 0.0);
        assert_eq!(normalize(10.0, r), 1.0);
        assert_eq!(normalize(5.0, r), 0.5);
        // Flat image → mid grey, no divide-by-zero.
        assert_eq!(normalize(3.0, (3.0, 3.0)), 0.5);
    }
}
