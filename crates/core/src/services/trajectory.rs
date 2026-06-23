//! Reconstruct **trajectory signals** from the scope3d column group a mflowLink
//! run produces.
//!
//! `signal_scope3d` logs its three inputs as a `base[x]` / `base[y]` / `base[z]`
//! column group (distinct from the numeric `base[1]`/`[2]`/`[3]` vector form), so
//! the IDE can plot the path the point traces rather than three separate time
//! series. Grouping those names by base recovers each trajectory. Pure +
//! GTK-free.

/// A 3-D trajectory recovered from a `base[x]`/`base[y]`(/`base[z]`) name group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrajectorySignal {
    pub base: String,
    /// True when a `base[z]` column is present (else the path is planar).
    pub has_z: bool,
}

impl TrajectorySignal {
    pub fn axis_name(&self, axis: char) -> String {
        format!("{}[{}]", self.base, axis)
    }
}

/// Split `base[x]` → `("base", 'x')` for an axis letter `x`/`y`/`z`.
fn parse_axis(name: &str) -> Option<(&str, char)> {
    let inner = name.strip_suffix(']')?;
    let open = inner.rfind('[')?;
    let axis = inner.get(open + 1..)?;
    let base = &name[..open];
    if base.is_empty() {
        return None;
    }
    match axis {
        "x" => Some((base, 'x')),
        "y" => Some((base, 'y')),
        "z" => Some((base, 'z')),
        _ => None,
    }
}

/// Group `base[x]`/`base[y]`/`base[z]` names into trajectories. A base needs at
/// least its `x` and `y` columns to count (z is optional); other signals are
/// ignored.
pub fn detect_trajectory_signals(names: &[String]) -> Vec<TrajectorySignal> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, (bool, bool, bool)> = BTreeMap::new();
    for name in names {
        if let Some((base, axis)) = parse_axis(name) {
            let e = groups
                .entry(base.to_string())
                .or_insert((false, false, false));
            match axis {
                'x' => e.0 = true,
                'y' => e.1 = true,
                _ => e.2 = true,
            }
        }
    }
    groups
        .into_iter()
        .filter(|(_, (x, y, _))| *x && *y)
        .map(|(base, (_, _, z))| TrajectorySignal { base, has_z: z })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn detects_xy_and_xyz_trajectories() {
        let t = detect_trajectory_signals(&names(&[
            "path[x]",
            "path[y]",
            "path[z]", // 3-D
            "ground[x]",
            "ground[y]", // planar
            "scope",
            "v[1]",
            "v[2]", // not trajectories
        ]));
        assert_eq!(t.len(), 2);
        let p = t.iter().find(|s| s.base == "path").unwrap();
        assert!(p.has_z);
        assert_eq!(p.axis_name('x'), "path[x]");
        let g = t.iter().find(|s| s.base == "ground").unwrap();
        assert!(!g.has_z);
    }

    #[test]
    fn needs_both_x_and_y() {
        // x alone (or y alone) is not a trajectory.
        assert!(detect_trajectory_signals(&names(&["a[x]", "b[y]"])).is_empty());
        assert!(detect_trajectory_signals(&names(&["a[x]", "a[z]"])).is_empty());
    }
}
