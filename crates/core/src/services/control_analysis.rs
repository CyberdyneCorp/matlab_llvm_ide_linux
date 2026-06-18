//! Linear-analysis of a SISO transfer function `num/den` (coefficients
//! highest-order first) — frequency response (Bode / Nyquist) and unit-step
//! response — computed directly in Rust. No compiler round-trip: Bode/Nyquist
//! are `H(jω) = num(jω)/den(jω)` complex evaluation, and the step response is
//! RK4 on the controllable-canonical realisation. Drives the Control-analysis
//! panel for Transfer Fcn / PID blocks (the IDE's Control System surface that
//! mirrors `matlabc`'s shipped `tf`/`bode`/`step`).

use crate::models::flowchart::{FlowNode, NodeKind};

/// A SISO transfer function: `num`/`den` polynomials, highest-order first.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferFunction {
    pub num: Vec<f64>,
    pub den: Vec<f64>,
}

/// One Bode/Nyquist sample at angular frequency `w` (rad/s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreqPoint {
    pub w: f64,
    pub mag_db: f64,
    pub phase_deg: f64,
    pub re: f64,
    pub im: f64,
}

/// A minimal complex number — just enough for polynomial evaluation.
#[derive(Clone, Copy)]
struct Cx {
    re: f64,
    im: f64,
}

impl Cx {
    fn new(re: f64, im: f64) -> Cx {
        Cx { re, im }
    }
    fn mul(self, o: Cx) -> Cx {
        Cx::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
    fn add(self, o: Cx) -> Cx {
        Cx::new(self.re + o.re, self.im + o.im)
    }
    fn div(self, o: Cx) -> Cx {
        let d = o.re * o.re + o.im * o.im;
        Cx::new(
            (self.re * o.re + self.im * o.im) / d,
            (self.im * o.re - self.re * o.im) / d,
        )
    }
    fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
    fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }
}

/// Evaluate a real polynomial (highest-order first) at complex `s` via Horner.
fn poly_eval(coeffs: &[f64], s: Cx) -> Cx {
    let mut acc = Cx::new(0.0, 0.0);
    for &c in coeffs {
        acc = acc.mul(s).add(Cx::new(c, 0.0));
    }
    acc
}

impl TransferFunction {
    /// Build the transfer function a `signal_*` block represents, if it is one
    /// the analysis panel can handle. Today: Transfer Fcn. (PID expands to
    /// `Kp + Ki/s + Kd·N/(s+N)` and State-Space to `C(sI−A)⁻¹B + D` — added as
    /// those block kinds land in the editor.)
    pub fn from_node(node: &FlowNode) -> Option<TransferFunction> {
        match node.kind {
            NodeKind::SignalTransferFcn => {
                let num = parse_coeffs(&node.param_str("num").unwrap_or_else(|| "1".into()))?;
                let den = parse_coeffs(&node.param_str("den").unwrap_or_else(|| "1".into()))?;
                Some(TransferFunction { num, den })
            }
            _ => None,
        }
    }

    /// Bode/Nyquist samples over `points` log-spaced frequencies in
    /// `[w_min, w_max]` rad/s. Phase is unwrapped for a continuous curve.
    pub fn frequency_response(&self, w_min: f64, w_max: f64, points: usize) -> Vec<FreqPoint> {
        let points = points.max(2);
        let (lo, hi) = (w_min.max(1e-12).log10(), w_max.max(1e-12).log10());
        let mut out = Vec::with_capacity(points);
        let mut prev_phase = 0.0_f64;
        for i in 0..points {
            let w = 10f64.powf(lo + (hi - lo) * (i as f64) / ((points - 1) as f64));
            let s = Cx::new(0.0, w);
            let h = poly_eval(&self.num, s).div(poly_eval(&self.den, s));
            let mag = h.abs();
            let mut phase = h.arg().to_degrees();
            if i > 0 {
                // Unwrap: keep within ±180° of the previous sample.
                while phase - prev_phase > 180.0 {
                    phase -= 360.0;
                }
                while phase - prev_phase < -180.0 {
                    phase += 360.0;
                }
            }
            prev_phase = phase;
            out.push(FreqPoint {
                w,
                mag_db: 20.0 * mag.max(1e-300).log10(),
                phase_deg: phase,
                re: h.re,
                im: h.im,
            });
        }
        out
    }

    /// Unit-step response sampled at `dt` over `[0, t_end]`, as `(t, y)` pairs.
    /// Integrates the controllable-canonical state space with RK4.
    pub fn step_response(&self, t_end: f64, dt: f64) -> Vec<(f64, f64)> {
        let dt = if dt > 0.0 { dt } else { t_end / 200.0 };
        // Trim leading zeros so the leading coefficient is the true order.
        let den: Vec<f64> = trim_leading(&self.den);
        let num: Vec<f64> = trim_leading(&self.num);
        if den.is_empty() {
            return Vec::new();
        }
        let lead = den[0];
        let order = den.len() - 1;
        // Static gain (no dynamics): constant output num/den at the DC point.
        if order == 0 {
            let g = if num.is_empty() { 0.0 } else { num[0] / lead };
            let steps = ((t_end / dt).ceil() as usize).max(1);
            return (0..=steps).map(|i| (i as f64 * dt, g)).collect();
        }
        // Normalise to a monic denominator and left-pad the numerator to n+1.
        let d: Vec<f64> = den.iter().map(|c| c / lead).collect(); // [1, d1, …, dn]
        let mut e = vec![0.0; order + 1];
        let pad = order + 1 - num.len();
        for (i, &c) in num.iter().enumerate() {
            e[pad + i] = c / lead;
        }
        let feed = e[0]; // D term (non-zero only when deg num == deg den)
                         // Strictly-proper numerator p_k = e_k − D·d_k, k = 1..n.
        let p: Vec<f64> = (1..=order).map(|k| e[k] - feed * d[k]).collect();

        let deriv = |x: &[f64], u: f64| -> Vec<f64> {
            let n = x.len();
            let mut dx = vec![0.0; n];
            // Companion-form shift: x_k' = x_{k+1} for k < n−1.
            dx[..n - 1].copy_from_slice(&x[1..n]);
            // x_n' = u − Σ d_{n−j}·x_j
            let mut last = u;
            for (j, xj) in x.iter().enumerate() {
                last -= d[order - j] * xj;
            }
            dx[n - 1] = last;
            dx
        };
        let output = |x: &[f64], u: f64| -> f64 {
            // y = Σ p_{n−j}·x_j + D·u
            let mut y = feed * u;
            for (j, xj) in x.iter().enumerate() {
                y += p[order - 1 - j] * xj;
            }
            y
        };

        let u = 1.0; // unit step
        let mut x = vec![0.0; order];
        let steps = ((t_end / dt).ceil() as usize).max(1);
        let mut out = Vec::with_capacity(steps + 1);
        out.push((0.0, output(&x, u)));
        for i in 0..steps {
            let k1 = deriv(&x, u);
            let x2: Vec<f64> = x.iter().zip(&k1).map(|(a, b)| a + 0.5 * dt * b).collect();
            let k2 = deriv(&x2, u);
            let x3: Vec<f64> = x.iter().zip(&k2).map(|(a, b)| a + 0.5 * dt * b).collect();
            let k3 = deriv(&x3, u);
            let x4: Vec<f64> = x.iter().zip(&k3).map(|(a, b)| a + dt * b).collect();
            let k4 = deriv(&x4, u);
            for j in 0..order {
                x[j] += dt / 6.0 * (k1[j] + 2.0 * k2[j] + 2.0 * k3[j] + k4[j]);
            }
            out.push(((i + 1) as f64 * dt, output(&x, u)));
        }
        out
    }
}

/// Drop leading zero coefficients (so a `"0, 1, 1"` denominator reads as order 2).
fn trim_leading(coeffs: &[f64]) -> Vec<f64> {
    let first = coeffs
        .iter()
        .position(|c| *c != 0.0)
        .unwrap_or(coeffs.len());
    coeffs[first..].to_vec()
}

/// Parse a comma/space-separated coefficient list. Returns `None` if empty or
/// any token is non-numeric.
fn parse_coeffs(s: &str) -> Option<Vec<f64>> {
    let v: Vec<f64> = s
        .split([',', ' ', '\t'])
        .filter(|t| !t.is_empty())
        .map(|t| t.parse::<f64>())
        .collect::<Result<_, _>>()
        .ok()?;
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::flowchart::{FlowUi, NodeData, ParamValue};
    use std::collections::BTreeMap;

    fn tf(num: &[f64], den: &[f64]) -> TransferFunction {
        TransferFunction {
            num: num.to_vec(),
            den: den.to_vec(),
        }
    }

    #[test]
    fn first_order_bode_known_points() {
        // 1/(s+1): DC gain 0 dB, and at ω = 1 the −3 dB / −45° corner.
        let r = tf(&[1.0], &[1.0, 1.0]).frequency_response(0.01, 100.0, 2001);
        let dc = r.first().unwrap();
        assert!(dc.mag_db.abs() < 0.01, "DC mag {} dB", dc.mag_db);
        let corner = r
            .iter()
            .min_by(|a, b| (a.w - 1.0).abs().total_cmp(&(b.w - 1.0).abs()))
            .unwrap();
        assert!(
            (corner.mag_db - -3.0103).abs() < 0.05,
            "corner mag {}",
            corner.mag_db
        );
        assert!(
            (corner.phase_deg - -45.0).abs() < 0.5,
            "corner phase {}",
            corner.phase_deg
        );
    }

    #[test]
    fn first_order_step_matches_analytic() {
        // 1/(s+1) unit step → 1 − e^{−t}.
        let y = tf(&[1.0], &[1.0, 1.0]).step_response(6.0, 0.001);
        let at = |t: f64| {
            y.iter()
                .min_by(|a, b| (a.0 - t).abs().total_cmp(&(b.0 - t).abs()))
                .unwrap()
                .1
        };
        assert!(
            (at(1.0) - (1.0 - (-1.0_f64).exp())).abs() < 1e-3,
            "y(1)={}",
            at(1.0)
        );
        assert!((at(6.0) - 1.0).abs() < 0.01, "y(6)={}", at(6.0));
        assert!(y[0].1.abs() < 1e-9, "y(0)={}", y[0].1);
    }

    #[test]
    fn integrator_den_with_leading_zero_is_trimmed() {
        // den "0, 1, 1" should read as (s+1), not crash on the leading zero.
        let r = tf(&[1.0], &[0.0, 1.0, 1.0]).frequency_response(0.1, 10.0, 51);
        assert!(r.iter().all(|p| p.mag_db.is_finite()));
    }

    #[test]
    fn nyquist_first_order_origin_and_dc() {
        let r = tf(&[1.0], &[1.0, 1.0]).frequency_response(0.001, 1000.0, 4001);
        // Low frequency → (1, 0); high frequency → (0, 0).
        assert!((r.first().unwrap().re - 1.0).abs() < 0.01);
        assert!(r.last().unwrap().re.abs() < 0.01 && r.last().unwrap().im.abs() < 0.01);
    }

    fn node(kind: NodeKind, params: &[(&str, ParamValue)]) -> FlowNode {
        let mut data = NodeData::default();
        let mut map = BTreeMap::new();
        for (k, v) in params {
            map.insert(k.to_string(), v.clone());
        }
        data.params = Some(map);
        FlowNode::new(
            "n",
            kind,
            "n",
            kind.default_ports(),
            data,
            FlowUi::default(),
        )
    }

    #[test]
    fn from_node_transfer_fcn() {
        let n = node(
            NodeKind::SignalTransferFcn,
            &[
                ("num", ParamValue::Str("1".into())),
                ("den", ParamValue::Str("1, 2, 1".into())),
            ],
        );
        let t = TransferFunction::from_node(&n).unwrap();
        assert_eq!(t.num, vec![1.0]);
        assert_eq!(t.den, vec![1.0, 2.0, 1.0]);
    }

    #[test]
    fn from_node_rejects_non_tf_blocks() {
        let n = node(NodeKind::SignalGain, &[("gain", ParamValue::Double(2.0))]);
        assert!(TransferFunction::from_node(&n).is_none());
    }
}
