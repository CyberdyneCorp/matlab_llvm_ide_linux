//! Palette taxonomy: `NodeCategory` (with per-dialect display orders and accent
//! colors) and `SignalFlowParamSpec` (the per-kind tunable-parameter lists the
//! inspector renders). Ported from `FlowchartModels.swift` + `Theme.swift`.

use crate::theme::{palette, Rgb};

use super::node::{NodeKind, ParamValue};

/// Palette section a node kind belongs to. The `&'static str` label matches the
/// reference's `rawValue` (used as the section header text).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeCategory {
    // Control-flow document categories
    ControlFlow,
    Data,
    Io,
    Functions,
    Matrix,
    Other,
    // Signal-flow document categories
    SignalSources,
    SignalSinks,
    SignalContinuous,
    SignalDiscrete,
    SignalMath,
    SignalRouting,
    SignalLookup,
    SignalComposite,
    SignalComms,
    SignalDsp,
    SignalHdl,
    // State-chart document categories
    ChartStates,
    ChartJunctions,
    ChartFunctions,
}

impl NodeCategory {
    /// Section header label (matches the reference's `rawValue`).
    pub fn label(self) -> &'static str {
        use NodeCategory::*;
        match self {
            ControlFlow => "Control Flow",
            Data => "Data",
            Io => "I/O",
            Functions => "Functions",
            Matrix => "Matrix",
            Other => "Other",
            SignalSources => "Sources",
            SignalSinks => "Sinks",
            SignalContinuous => "Continuous",
            SignalDiscrete => "Discrete",
            SignalMath => "Math",
            SignalRouting => "Signal Routing",
            SignalLookup => "Lookup Tables",
            SignalComposite => "Composite",
            SignalComms => "Communications",
            SignalDsp => "DSP & Image",
            SignalHdl => "HDL",
            ChartStates => "States",
            ChartJunctions => "Junctions",
            ChartFunctions => "Chart Functions",
        }
    }

    /// Accent color the palette stripes the section header with.
    pub fn accent(self) -> Rgb {
        use NodeCategory::*;
        match self {
            Other => palette::ACCENT_GREEN,
            ControlFlow => palette::ACCENT_MAGENTA,
            Data => palette::ACCENT_BLUE,
            Io => palette::ACCENT_YELLOW,
            Functions => palette::ACCENT_ORANGE,
            Matrix => palette::ACCENT_CYAN,
            SignalSources => palette::ACCENT_GREEN,
            SignalSinks => palette::ACCENT_RED,
            SignalContinuous => palette::ACCENT_BLUE,
            SignalDiscrete => palette::ACCENT_CYAN,
            SignalMath => palette::ACCENT_AMBER,
            SignalRouting => palette::ACCENT_MAGENTA,
            SignalLookup => palette::ACCENT_YELLOW,
            SignalComposite => palette::ACCENT_ORANGE,
            SignalComms => palette::ACCENT_BLUE,
            SignalDsp => palette::ACCENT_CYAN,
            SignalHdl => palette::ACCENT_GREEN,
            ChartStates => palette::ACCENT_ORANGE,
            ChartJunctions => palette::ACCENT_CYAN,
            ChartFunctions => palette::ACCENT_MAGENTA,
        }
    }

    pub fn is_signal_flow(self) -> bool {
        use NodeCategory::*;
        matches!(
            self,
            SignalSources
                | SignalSinks
                | SignalContinuous
                | SignalDiscrete
                | SignalMath
                | SignalRouting
                | SignalLookup
                | SignalComposite
                | SignalComms
                | SignalDsp
                | SignalHdl
        )
    }

    pub fn is_state_chart(self) -> bool {
        use NodeCategory::*;
        matches!(self, ChartStates | ChartJunctions | ChartFunctions)
    }

    /// Stable display order for the control-flow palette.
    pub fn control_flow_order() -> [NodeCategory; 6] {
        use NodeCategory::*;
        [Other, ControlFlow, Data, Io, Functions, Matrix]
    }

    /// Display order for the signal-flow palette.
    pub fn signal_flow_order() -> [NodeCategory; 11] {
        use NodeCategory::*;
        [
            SignalSources,
            SignalContinuous,
            SignalDiscrete,
            SignalMath,
            SignalDsp,
            SignalComms,
            SignalHdl,
            SignalRouting,
            SignalLookup,
            SignalSinks,
            SignalComposite,
        ]
    }

    /// Display order for the state-chart palette.
    pub fn state_chart_order() -> [NodeCategory; 3] {
        use NodeCategory::*;
        [ChartStates, ChartJunctions, ChartFunctions]
    }

    /// The dialect's display order (control-flow / signal-flow / state-chart).
    pub fn order_for(schema: super::document::SchemaKind) -> Vec<NodeCategory> {
        use super::document::SchemaKind;
        match schema {
            SchemaKind::ControlFlow => Self::control_flow_order().to_vec(),
            SchemaKind::SignalFlow => Self::signal_flow_order().to_vec(),
            SchemaKind::StateChart => Self::state_chart_order().to_vec(),
        }
    }
}

/// Every addable block for `schema`, grouped under its category in display order
/// (empty categories dropped). The structural `Start`/`End` scaffold is excluded
/// — they already live on the canvas. Drives the Block Library window.
pub fn library_blocks(schema: super::document::SchemaKind) -> Vec<(NodeCategory, Vec<NodeKind>)> {
    NodeCategory::order_for(schema)
        .into_iter()
        .filter_map(|cat| {
            let kinds: Vec<NodeKind> = NodeKind::ALL
                .iter()
                .copied()
                .filter(|k| k.category() == cat && !matches!(k, NodeKind::Start | NodeKind::End))
                .collect();
            (!kinds.is_empty()).then_some((cat, kinds))
        })
        .collect()
}

/// The kind of value a [`SignalFlowParamSpec`] field accepts. Drives inspector
/// validation so a malformed parameter is caught at edit time rather than later
/// inside `matlabc`.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamConstraint {
    /// Any finite real number.
    Number,
    /// A whole number with an optional inclusive minimum (e.g. input counts ≥ 1).
    Integer { min: Option<i64> },
    /// A comma/space-separated list of reals (polynomial coefficients).
    CoeffList,
    /// A MATLAB matrix literal (`[1 2; 3 4]`) or a scalar/coefficient row.
    Matrix,
    /// A sign string built only from `+` and `-` (e.g. Sum's `"+-"`).
    Signs,
    /// One of a fixed set of allowed strings (case-insensitive).
    Enum(&'static [&'static str]),
    /// Free-form text (names, titles, expressions); always valid.
    Text,
}

/// One signal-flow block parameter shown by the inspector.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalFlowParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub default_value: ParamValue,
    pub constraint: ParamConstraint,
}

impl SignalFlowParamSpec {
    fn d(key: &'static str, label: &'static str, v: f64) -> SignalFlowParamSpec {
        SignalFlowParamSpec {
            key,
            label,
            default_value: ParamValue::Double(v),
            constraint: ParamConstraint::Number,
        }
    }
    fn s(key: &'static str, label: &'static str, v: &str) -> SignalFlowParamSpec {
        SignalFlowParamSpec {
            key,
            label,
            default_value: ParamValue::Str(v.to_string()),
            constraint: ParamConstraint::Text,
        }
    }

    /// Override this field's constraint (builder style).
    fn with(mut self, constraint: ParamConstraint) -> SignalFlowParamSpec {
        self.constraint = constraint;
        self
    }
    fn int(self, min: i64) -> SignalFlowParamSpec {
        self.with(ParamConstraint::Integer { min: Some(min) })
    }
    fn coeffs(self) -> SignalFlowParamSpec {
        self.with(ParamConstraint::CoeffList)
    }
    fn matrix(self) -> SignalFlowParamSpec {
        self.with(ParamConstraint::Matrix)
    }
    fn signs(self) -> SignalFlowParamSpec {
        self.with(ParamConstraint::Signs)
    }
    fn choices(self, set: &'static [&'static str]) -> SignalFlowParamSpec {
        self.with(ParamConstraint::Enum(set))
    }

    /// Per-kind tunable parameter list (matches roadmap §4.3 / Simulink dialogs).
    /// Returns `[]` for kinds with no user-tunable parameters.
    pub fn fields(kind: NodeKind) -> Vec<SignalFlowParamSpec> {
        use NodeKind::*;
        const RELOP_OPS: &[&str] = &["<", "<=", ">", ">=", "==", "~="];
        const LOGICAL_OPS: &[&str] = &["AND", "OR", "NAND", "NOR", "XOR", "NOT"];
        const DISCRETE_METHODS: &[&str] = &["ForwardEuler", "BackwardEuler", "Trapezoidal"];
        const NOISE_DISTS: &[&str] = &["uniform", "gaussian", "normal"];
        const BOOL_CHOICES: &[&str] = &["false", "true"];
        const WINDOW_TYPES: &[&str] = &["hann", "hamming", "blackman", "rect"];
        const IMAGE_FILTERS: &[&str] = &["box", "gaussian3", "sobelx", "sobely"];
        const COLOR_MODES: &[&str] = &["rgb2gray", "gray2rgb"];
        const STATS: &[&str] = &["mean", "var", "std"];
        const ACTIVATIONS: &[&str] = &["relu", "tanh", "sigmoid", "linear"];
        const ACTION_TYPES: &[&str] = &["discrete", "continuous"];
        const INTERP: &[&str] = &["linear", "zoh"];
        match kind {
            SignalConstant => vec![Self::d("value", "Value", 1.0)],
            SignalStep => vec![
                Self::d("stepTime", "Step Time", 1.0),
                Self::d("initialValue", "Initial Value", 0.0),
                Self::d("finalValue", "Final Value", 1.0),
            ],
            SignalSine => vec![
                Self::d("amplitude", "Amplitude", 1.0),
                Self::d("bias", "Bias", 0.0),
                Self::d("frequency", "Frequency (rad/s)", 1.0),
                Self::d("phase", "Phase (rad)", 0.0),
            ],
            SignalPulse => vec![
                Self::d("amplitude", "Amplitude", 1.0),
                Self::d("period", "Period", 1.0),
                Self::d("pulseWidth", "Pulse Width (% period)", 50.0),
                Self::d("phaseDelay", "Phase Delay", 0.0),
            ],
            SignalRamp => vec![
                Self::d("slope", "Slope", 1.0),
                Self::d("startTime", "Start Time", 0.0),
                Self::d("initialOutput", "Initial Output", 0.0),
            ],
            SignalGain => vec![Self::d("gain", "Gain", 1.0)],
            SignalSum => vec![Self::s("signs", "List of Signs", "++").signs()],
            SignalProduct => vec![Self::d("numInputs", "Number of Inputs", 2.0).int(1)],
            SignalSaturation => vec![
                Self::d("upperLimit", "Upper Limit", 1.0),
                Self::d("lowerLimit", "Lower Limit", -1.0),
            ],
            SignalIntegrator => vec![Self::d("initialCondition", "Initial Condition", 0.0)],
            // Parallel-form PID: C(s) = Kp + Ki/s + Kd·N/(s+N). Optional output
            // saturation limits (upperLimit/lowerLimit) are honoured by the
            // compiler but omitted here — they need "empty = no limit" handling.
            SignalPid => vec![
                Self::d("Kp", "Proportional (Kp)", 1.0),
                Self::d("Ki", "Integral (Ki)", 0.0),
                Self::d("Kd", "Derivative (Kd)", 0.0),
                Self::d("N", "Filter coefficient (N)", 100.0),
                Self::d("initialIntegral", "Initial Integral", 0.0),
            ],
            SignalTransferFcn => vec![
                Self::s("num", "Numerator coeffs", "1").coeffs(),
                Self::s("den", "Denominator coeffs", "1, 1").coeffs(),
            ],
            SignalStateSpace => vec![
                Self::s("A", "A matrix", "0").matrix(),
                Self::s("B", "B matrix", "1").matrix(),
                Self::s("C", "C matrix", "1").matrix(),
                Self::d("D", "D feedthru", 0.0),
                Self::s("x0", "Initial state", "0").coeffs(),
            ],
            SignalUnitDelay => vec![
                Self::d("initialValue", "Initial Value", 0.0),
                Self::d("sampleTime", "Sample Time", 1.0),
            ],
            SignalZoh => vec![Self::d("sampleTime", "Sample Time", 1.0)],
            SignalScope => vec![
                Self::d("yMin", "Y min", -1.0),
                Self::d("yMax", "Y max", 1.0),
                Self::s("title", "Title", ""),
                Self::d("decimation", "Decimation", 1.0).int(1),
            ],
            SignalToWorkspace => vec![Self::s("variableName", "Variable name", "simout")],
            // From Workspace: an inline `t v; t v; …` time-series replayed at sim
            // time (matlab_llvm#388), interpolated linearly or held (zoh).
            SignalFromWorkspace => vec![
                Self::s("data", "Time-series (t v; …)", "0 0; 1 1").matrix(),
                Self::s("interpolation", "Interpolation", "linear").choices(INTERP),
            ],
            SignalMux => vec![Self::d("numInputs", "Number of Inputs", 2.0).int(1)],
            SignalDemux => vec![Self::d("numOutputs", "Number of Outputs", 2.0).int(1)],
            SignalSwitch => vec![Self::d("threshold", "Threshold", 0.0)],
            SignalChirp => vec![
                Self::d("amplitude", "Amplitude", 1.0),
                Self::d("f0", "f0 (Hz)", 0.1),
                Self::d("f1", "f1 (Hz)", 1.0),
                Self::d("t1", "Sweep end t1 (s)", 10.0),
            ],
            SignalNoise => vec![
                Self::d("amplitude", "Amplitude", 1.0),
                Self::d("seed", "Seed", 1.0).int(0),
                Self::s("kind", "Distribution", "uniform").choices(NOISE_DISTS),
            ],
            SignalFunctionCallGenerator => vec![
                Self::d("period", "Period", 1.0),
                Self::d("phaseDelay", "Phase Delay", 0.0),
            ],
            SignalZeroPole => vec![
                Self::s("zeros", "Zeros", "").coeffs(),
                Self::s("poles", "Poles", "-1").coeffs(),
                Self::d("gain", "Scalar Gain", 1.0),
            ],
            SignalTransportDelay => vec![
                Self::d("delay", "Delay (s)", 0.0),
                Self::d("initialOutput", "Initial Output", 0.0),
            ],
            SignalDiscreteIntegrator => vec![
                Self::s("method", "Method", "ForwardEuler").choices(DISCRETE_METHODS),
                Self::d("initialCondition", "Initial Condition", 0.0),
                Self::d("sampleTime", "Sample Time", 1.0),
            ],
            SignalDiscreteFilter => vec![
                Self::s("num", "Numerator coeffs", "1").coeffs(),
                Self::s("den", "Denominator coeffs", "1, -0.9").coeffs(),
                Self::d("sampleTime", "Sample Time", 1.0),
            ],
            SignalRateTransition => vec![Self::d("sampleTime", "Sample Time", 1.0)],
            SignalMathFcn => vec![Self::s("function", "Function", "sqrt")],
            SignalTrigFcn => vec![Self::s("function", "Function", "sin")],
            SignalDeadZone => vec![
                Self::d("lowerLimit", "Start of dead zone", -0.5),
                Self::d("upperLimit", "End of dead zone", 0.5),
            ],
            SignalRelop => vec![Self::s("op", "Operator", "<").choices(RELOP_OPS)],
            SignalLogical => vec![Self::s("op", "Operator", "AND").choices(LOGICAL_OPS)],
            SignalCompareToZero => vec![Self::s("op", "Operator", ">").choices(RELOP_OPS)],
            SignalBusCreator => vec![Self::s("field_names", "Field Names", "")],
            SignalBusSelector => vec![Self::s("field", "Field", "")],
            SignalReshape => vec![
                Self::d("rows", "Rows", 1.0).int(1),
                Self::d("cols", "Cols", 1.0).int(1),
                Self::s("shape", "Shape (alt form)", ""),
            ],
            SignalMatlabFcn => vec![
                Self::s("expression", "Expression", "u"),
                Self::s("function_body", "Function Body", ""),
            ],
            // Communications (#343)
            SignalAwgn => vec![
                Self::d("snr", "SNR (dB)", 10.0),
                Self::d("signalPower", "Signal Power", 1.0),
                Self::d("seed", "Seed", 1.0).int(0),
            ],
            SignalPskMod | SignalPskDemod => vec![
                Self::d("M", "M (order)", 4.0).int(2),
                Self::d("phaseOffset", "Phase Offset (rad)", 0.0),
            ],
            SignalQamMod | SignalQamDemod => vec![
                Self::d("M", "M (order)", 16.0).int(2),
                Self::s("normalize", "Normalize Power", "false").choices(BOOL_CHOICES),
            ],
            SignalErrorRate => vec![Self::d("tolerance", "Tolerance", 0.5)],
            // DSP & image (#343)
            SignalFft | SignalIfft | SignalSpectrum | SignalDwt | SignalIdwt => {
                vec![Self::d("n", "Frame Size (n)", 8.0).int(1)]
            }
            SignalWindow => vec![
                Self::d("n", "Frame Size (n)", 8.0).int(1),
                Self::s("type", "Window Type", "hann").choices(WINDOW_TYPES),
            ],
            SignalBiquad => vec![
                Self::s("b", "Numerator b", "1 0 0").coeffs(),
                Self::s("a", "Denominator a", "1 0 0").coeffs(),
                Self::d("sampleTime", "Sample Time", 1.0),
            ],
            SignalLowpass => vec![
                Self::d("alpha", "Alpha", 0.1),
                Self::d("sampleTime", "Sample Time", 1.0),
            ],
            SignalHighpass => vec![
                Self::d("alpha", "Alpha", 0.9),
                Self::d("sampleTime", "Sample Time", 1.0),
            ],
            SignalDcBlock => vec![
                Self::d("r", "Pole r", 0.995),
                Self::d("sampleTime", "Sample Time", 1.0),
            ],
            SignalImageFilter => vec![
                Self::s("type", "Filter Type", "box").choices(IMAGE_FILTERS),
                Self::s("kernel", "Kernel (matrix)", "").matrix(),
            ],
            SignalColorSpace => vec![Self::s("mode", "Mode", "rgb2gray").choices(COLOR_MODES)],
            SignalThreshold => vec![Self::d("level", "Level", 0.5)],
            // HDL / digital sequential (#343)
            SignalDff | SignalTff | SignalJkff | SignalSrff => {
                vec![Self::d("initialValue", "Initial Value", 0.0)]
            }
            SignalCounter => vec![
                Self::d("step", "Step", 1.0),
                Self::d("modulus", "Modulus (0 = none)", 0.0),
            ],
            SignalShiftRegister => vec![
                Self::d("length", "Length", 4.0).int(1),
                Self::d("initialValue", "Initial Value", 0.0),
            ],
            SignalRam => vec![
                Self::d("depth", "Depth", 8.0).int(1),
                Self::d("initialValue", "Initial Value", 0.0),
            ],
            SignalRom => vec![Self::s("content", "Content (vector)", "1 2 3").coeffs()],
            // Additional sources (#343)
            SignalRepeatingSequence => vec![
                Self::s("timeValues", "Time Values", "0 1").coeffs(),
                Self::s("outputValues", "Output Values", "0 1").coeffs(),
            ],
            SignalImageSource => vec![
                Self::d("rows", "Rows", 3.0).int(1),
                Self::d("cols", "Cols", 3.0).int(1),
                Self::s("data", "Pixel Data (row-major)", "0 0 0 0 1 0 0 0 0").coeffs(),
            ],
            // Estimation / ML / control (#343)
            SignalKalman => vec![
                Self::s("A", "A matrix", "1").matrix(),
                Self::s("C", "C matrix", "1").matrix(),
                Self::s("Q", "Process noise Q", "0.01").matrix(),
                Self::s("R", "Measurement noise R", "1").matrix(),
                Self::s("B", "B matrix (opt)", "").matrix(),
                Self::s("x0", "Initial state x0", "0").matrix(),
                Self::s("P0", "Initial cov P0", "1").matrix(),
            ],
            SignalLqr => vec![
                Self::s("K", "Gain K (matrix)", "1").matrix(),
                Self::d("sign", "Sign (+1 / -1)", -1.0),
            ],
            SignalRunningStats => vec![Self::s("stat", "Statistic", "mean").choices(STATS)],
            SignalDnnPredict => vec![
                Self::s("W1", "W1 matrix", "1").matrix(),
                Self::s("b1", "b1 vector", "0").matrix(),
                Self::s("W2", "W2 matrix", "1").matrix(),
                Self::s("b2", "b2 vector", "0").matrix(),
                Self::s("activation", "Activation", "relu").choices(ACTIVATIONS),
            ],
            SignalRlAgent => vec![
                Self::s("W1", "W1 matrix", "1").matrix(),
                Self::s("b1", "b1 vector", "0").matrix(),
                Self::s("W2", "W2 matrix", "1").matrix(),
                Self::s("b2", "b2 vector", "0").matrix(),
                Self::s("actionType", "Action Type", "discrete").choices(ACTION_TYPES),
                Self::d("actionScale", "Action Scale", 1.0),
            ],
            SignalRf2Port => vec![Self::s("S", "S-parameters (2x2)", "0 1; 1 0").matrix()],
            SignalPoseTransform => vec![
                Self::d("x", "x", 0.0),
                Self::d("y", "y", 0.0),
                Self::d("theta", "theta (rad)", 0.0),
            ],
            _ => vec![],
        }
    }

    /// Validate `input` for the parameter `key` of `kind`. Unknown keys are
    /// accepted (no spec to check against).
    pub fn validate_field(kind: NodeKind, key: &str, input: &str) -> Result<(), String> {
        match Self::fields(kind).into_iter().find(|f| f.key == key) {
            Some(spec) => spec.validate(input),
            None => Ok(()),
        }
    }

    /// Validate `input` against this field's constraint. Empty input is valid —
    /// the inspector treats it as "clear the parameter".
    pub fn validate(&self, input: &str) -> Result<(), String> {
        let t = input.trim();
        if t.is_empty() {
            return Ok(());
        }
        match &self.constraint {
            ParamConstraint::Text => Ok(()),
            ParamConstraint::Number => parse_real(t)
                .map(|_| ())
                .ok_or_else(|| format!("“{t}” is not a number")),
            ParamConstraint::Integer { min } => {
                let v = parse_real(t).ok_or_else(|| format!("“{t}” is not a number"))?;
                if v.fract() != 0.0 {
                    return Err(format!("“{t}” must be a whole number"));
                }
                match min {
                    Some(m) if (v as i64) < *m => Err(format!("must be ≥ {m}")),
                    _ => Ok(()),
                }
            }
            ParamConstraint::CoeffList => validate_coeff_list(t),
            ParamConstraint::Matrix => validate_matrix(t),
            ParamConstraint::Signs => {
                if t.chars().all(|c| c == '+' || c == '-') {
                    Ok(())
                } else {
                    Err("only “+” and “-” are allowed".to_string())
                }
            }
            ParamConstraint::Enum(set) => {
                if set.iter().any(|s| s.eq_ignore_ascii_case(t)) {
                    Ok(())
                } else {
                    Err(format!("must be one of: {}", set.join(", ")))
                }
            }
        }
    }
}

/// Parse a finite real number (rejects `inf`/`NaN`).
fn parse_real(s: &str) -> Option<f64> {
    let v: f64 = s.trim().parse().ok()?;
    v.is_finite().then_some(v)
}

/// Validate a comma/space-separated list of reals (polynomial coefficients).
fn validate_coeff_list(s: &str) -> Result<(), String> {
    let toks: Vec<&str> = s
        .split([',', ' ', '\t'])
        .filter(|t| !t.is_empty())
        .collect();
    if toks.is_empty() {
        return Err("expected at least one coefficient".to_string());
    }
    for t in toks {
        if parse_real(t).is_none() {
            return Err(format!("“{t}” is not a number"));
        }
    }
    Ok(())
}

/// Validate a MATLAB matrix literal (`[1 2; 3 4]`) or a bare scalar/row.
fn validate_matrix(s: &str) -> Result<(), String> {
    let inner = s
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .unwrap_or(s);
    if inner.trim().is_empty() {
        return Err("matrix is empty".to_string());
    }
    for row in inner.split(';') {
        validate_coeff_list(row).map_err(|_| "matrix entries must be numbers".to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_match_reference() {
        assert_eq!(NodeCategory::Io.label(), "I/O");
        assert_eq!(NodeCategory::SignalRouting.label(), "Signal Routing");
        assert_eq!(NodeCategory::ChartFunctions.label(), "Chart Functions");
    }

    #[test]
    fn library_groups_blocks_by_dialect() {
        use super::super::document::SchemaKind;

        // Signal-flow shows many blocks across its categories, all signal-flow,
        // ordered with Sources first; no structural Start/End leak in.
        let sig = library_blocks(SchemaKind::SignalFlow);
        assert!(
            sig.len() >= 4,
            "expected several signal categories, got {}",
            sig.len()
        );
        assert_eq!(sig[0].0, NodeCategory::SignalSources);
        let total: usize = sig.iter().map(|(_, ks)| ks.len()).sum();
        assert!(
            total > 6,
            "library should list more than the curated palette ({total})"
        );
        for (cat, kinds) in &sig {
            assert!(cat.is_signal_flow());
            assert!(!kinds.is_empty());
            assert!(kinds
                .iter()
                .all(|k| !matches!(k, NodeKind::Start | NodeKind::End)));
        }

        // Control-flow and state-chart produce their own non-empty groupings.
        assert!(!library_blocks(SchemaKind::ControlFlow).is_empty());
        let chart = library_blocks(SchemaKind::StateChart);
        assert!(chart.iter().all(|(c, _)| c.is_state_chart()));
    }

    #[test]
    fn dialect_predicates() {
        assert!(NodeCategory::SignalMath.is_signal_flow());
        assert!(!NodeCategory::Data.is_signal_flow());
        assert!(NodeCategory::ChartStates.is_state_chart());
        assert!(!NodeCategory::SignalMath.is_state_chart());
    }

    #[test]
    fn display_orders_are_complete() {
        assert_eq!(NodeCategory::control_flow_order().len(), 6);
        assert_eq!(NodeCategory::signal_flow_order().len(), 11);
        assert_eq!(NodeCategory::state_chart_order().len(), 3);
        // signal order starts with Sources, ends with Composite
        let order = NodeCategory::signal_flow_order();
        assert_eq!(order[0], NodeCategory::SignalSources);
        assert_eq!(*order.last().unwrap(), NodeCategory::SignalComposite);
    }

    #[test]
    fn accent_colors_are_assigned() {
        assert_eq!(NodeCategory::ControlFlow.accent(), palette::ACCENT_MAGENTA);
        assert_eq!(NodeCategory::SignalSinks.accent(), palette::ACCENT_RED);
    }

    #[test]
    fn every_category_has_label_accent_and_one_dialect() {
        use NodeCategory::*;
        let all = [
            ControlFlow,
            Data,
            Io,
            Functions,
            Matrix,
            Other,
            SignalSources,
            SignalSinks,
            SignalContinuous,
            SignalDiscrete,
            SignalMath,
            SignalRouting,
            SignalLookup,
            SignalComposite,
            SignalComms,
            SignalDsp,
            SignalHdl,
            ChartStates,
            ChartJunctions,
            ChartFunctions,
        ];
        for c in all {
            assert!(!c.label().is_empty(), "{c:?} has no label");
            let _ = c.accent(); // every arm returns a color
                                // Signal/state predicates partition the dialect-specific categories.
            assert!(
                !(c.is_signal_flow() && c.is_state_chart()),
                "{c:?} in two dialects"
            );
        }
        // The control-flow categories are neither signal nor chart.
        for c in [ControlFlow, Data, Io, Functions, Matrix, Other] {
            assert!(!c.is_signal_flow() && !c.is_state_chart());
        }
    }

    #[test]
    fn param_fields_for_known_blocks() {
        let gain = SignalFlowParamSpec::fields(NodeKind::SignalGain);
        assert_eq!(gain.len(), 1);
        assert_eq!(gain[0].key, "gain");
        assert_eq!(gain[0].default_value, ParamValue::Double(1.0));

        let sine = SignalFlowParamSpec::fields(NodeKind::SignalSine);
        assert_eq!(sine.len(), 4);

        let sum = SignalFlowParamSpec::fields(NodeKind::SignalSum);
        assert_eq!(sum[0].default_value, ParamValue::Str("++".into()));

        // PID exposes the gains + filter coefficient; keys match the compiler's
        // signal_pid evaluator (docs/mflowlink_blocks.md).
        let pid = SignalFlowParamSpec::fields(NodeKind::SignalPid);
        let keys: Vec<&str> = pid.iter().map(|f| f.key).collect();
        assert_eq!(keys, ["Kp", "Ki", "Kd", "N", "initialIntegral"]);
        assert!(pid
            .iter()
            .all(|f| f.validate(&f.default_value.display_string()).is_ok()));
    }

    #[test]
    fn pid_is_continuous_and_not_a_loop_breaker() {
        // Direct-feedthrough (Kp + Kd·N path), so PID must NOT break a loop.
        assert!(NodeKind::SignalPid.is_signal_flow());
        assert_eq!(
            NodeKind::SignalPid.category(),
            NodeCategory::SignalContinuous
        );
        assert!(!NodeKind::SignalPid.breaks_algebraic_loop());
    }

    #[test]
    fn param_fields_empty_for_parameterless_blocks() {
        assert!(SignalFlowParamSpec::fields(NodeKind::SignalAbs).is_empty());
        assert!(SignalFlowParamSpec::fields(NodeKind::SignalTerminator).is_empty());
        assert!(SignalFlowParamSpec::fields(NodeKind::Assignment).is_empty());
    }

    fn spec(c: ParamConstraint) -> SignalFlowParamSpec {
        SignalFlowParamSpec {
            key: "k",
            label: "K",
            default_value: ParamValue::Str(String::new()),
            constraint: c,
        }
    }

    #[test]
    fn validate_number_and_integer() {
        assert!(spec(ParamConstraint::Number).validate("3.14").is_ok());
        assert!(spec(ParamConstraint::Number).validate("-2e-3").is_ok());
        assert!(spec(ParamConstraint::Number).validate("abc").is_err());
        assert!(spec(ParamConstraint::Number).validate("inf").is_err());
        let int1 = spec(ParamConstraint::Integer { min: Some(1) });
        assert!(int1.validate("2").is_ok());
        assert!(int1.validate("0").is_err()); // below min
        assert!(int1.validate("1.5").is_err()); // not whole
    }

    #[test]
    fn validate_coeff_matrix_signs_enum() {
        assert!(spec(ParamConstraint::CoeffList).validate("1, 2, 3").is_ok());
        assert!(spec(ParamConstraint::CoeffList).validate("1 2 3").is_ok());
        assert!(spec(ParamConstraint::CoeffList)
            .validate("1, x, 3")
            .is_err());
        assert!(spec(ParamConstraint::Matrix).validate("[1 2; 3 4]").is_ok());
        assert!(spec(ParamConstraint::Matrix).validate("0").is_ok());
        assert!(spec(ParamConstraint::Matrix).validate("[1 a]").is_err());
        assert!(spec(ParamConstraint::Signs).validate("+-+").is_ok());
        assert!(spec(ParamConstraint::Signs).validate("+x").is_err());
        let e = spec(ParamConstraint::Enum(&["AND", "OR"]));
        assert!(e.validate("and").is_ok()); // case-insensitive
        assert!(e.validate("XOR").is_err());
    }

    #[test]
    fn empty_input_is_always_valid() {
        assert!(spec(ParamConstraint::Number).validate("").is_ok());
        assert!(spec(ParamConstraint::CoeffList).validate("   ").is_ok());
        assert!(spec(ParamConstraint::Enum(&["AND"])).validate("").is_ok());
    }

    #[test]
    fn every_default_parameter_validates() {
        for kind in NodeKind::ALL {
            for spec in SignalFlowParamSpec::fields(kind) {
                let v = spec.default_value.display_string();
                assert!(
                    spec.validate(&v).is_ok(),
                    "{:?}.{} default {:?} should validate",
                    kind,
                    spec.key,
                    v
                );
            }
        }
    }

    #[test]
    fn validate_field_by_kind_key() {
        assert!(SignalFlowParamSpec::validate_field(NodeKind::SignalGain, "gain", "2").is_ok());
        assert!(SignalFlowParamSpec::validate_field(NodeKind::SignalGain, "gain", "x").is_err());
        assert!(
            SignalFlowParamSpec::validate_field(NodeKind::SignalProduct, "numInputs", "0").is_err()
        );
        // Unknown key → accepted (no spec).
        assert!(SignalFlowParamSpec::validate_field(NodeKind::SignalGain, "nope", "x").is_ok());
    }
}
