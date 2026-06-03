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
    pub fn signal_flow_order() -> [NodeCategory; 8] {
        use NodeCategory::*;
        [
            SignalSources,
            SignalContinuous,
            SignalDiscrete,
            SignalMath,
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
pub fn library_blocks(
    schema: super::document::SchemaKind,
) -> Vec<(NodeCategory, Vec<NodeKind>)> {
    NodeCategory::order_for(schema)
        .into_iter()
        .filter_map(|cat| {
            let kinds: Vec<NodeKind> = NodeKind::ALL
                .iter()
                .copied()
                .filter(|k| {
                    k.category() == cat && !matches!(k, NodeKind::Start | NodeKind::End)
                })
                .collect();
            (!kinds.is_empty()).then_some((cat, kinds))
        })
        .collect()
}

/// One signal-flow block parameter shown by the inspector.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalFlowParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub default_value: ParamValue,
}

impl SignalFlowParamSpec {
    fn d(key: &'static str, label: &'static str, v: f64) -> SignalFlowParamSpec {
        SignalFlowParamSpec { key, label, default_value: ParamValue::Double(v) }
    }
    fn s(key: &'static str, label: &'static str, v: &str) -> SignalFlowParamSpec {
        SignalFlowParamSpec { key, label, default_value: ParamValue::Str(v.to_string()) }
    }

    /// Per-kind tunable parameter list (matches roadmap §4.3 / Simulink dialogs).
    /// Returns `[]` for kinds with no user-tunable parameters.
    pub fn fields(kind: NodeKind) -> Vec<SignalFlowParamSpec> {
        use NodeKind::*;
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
            SignalSum => vec![Self::s("signs", "List of Signs", "++")],
            SignalProduct => vec![Self::d("numInputs", "Number of Inputs", 2.0)],
            SignalSaturation => vec![
                Self::d("upperLimit", "Upper Limit", 1.0),
                Self::d("lowerLimit", "Lower Limit", -1.0),
            ],
            SignalIntegrator => vec![Self::d("initialCondition", "Initial Condition", 0.0)],
            SignalTransferFcn => vec![
                Self::s("num", "Numerator coeffs", "1"),
                Self::s("den", "Denominator coeffs", "1, 1"),
            ],
            SignalStateSpace => vec![
                Self::s("A", "A matrix", "0"),
                Self::s("B", "B matrix", "1"),
                Self::s("C", "C matrix", "1"),
                Self::d("D", "D feedthru", 0.0),
                Self::s("x0", "Initial state", "0"),
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
                Self::d("decimation", "Decimation", 1.0),
            ],
            SignalToWorkspace => vec![Self::s("variableName", "Variable name", "simout")],
            SignalMux => vec![Self::d("numInputs", "Number of Inputs", 2.0)],
            SignalDemux => vec![Self::d("numOutputs", "Number of Outputs", 2.0)],
            SignalSwitch => vec![Self::d("threshold", "Threshold", 0.0)],
            SignalChirp => vec![
                Self::d("amplitude", "Amplitude", 1.0),
                Self::d("f0", "f0 (Hz)", 0.1),
                Self::d("f1", "f1 (Hz)", 1.0),
                Self::d("t1", "Sweep end t1 (s)", 10.0),
            ],
            SignalNoise => vec![
                Self::d("amplitude", "Amplitude", 1.0),
                Self::d("seed", "Seed", 1.0),
                Self::s("kind", "Distribution", "uniform"),
            ],
            SignalFunctionCallGenerator => vec![
                Self::d("period", "Period", 1.0),
                Self::d("phaseDelay", "Phase Delay", 0.0),
            ],
            SignalZeroPole => vec![
                Self::s("zeros", "Zeros", ""),
                Self::s("poles", "Poles", "-1"),
                Self::d("gain", "Scalar Gain", 1.0),
            ],
            SignalTransportDelay => vec![
                Self::d("delay", "Delay (s)", 0.0),
                Self::d("initialOutput", "Initial Output", 0.0),
            ],
            SignalDiscreteIntegrator => vec![
                Self::s("method", "Method", "ForwardEuler"),
                Self::d("initialCondition", "Initial Condition", 0.0),
                Self::d("sampleTime", "Sample Time", 1.0),
            ],
            SignalDiscreteFilter => vec![
                Self::s("num", "Numerator coeffs", "1"),
                Self::s("den", "Denominator coeffs", "1, -0.9"),
                Self::d("sampleTime", "Sample Time", 1.0),
            ],
            SignalRateTransition => vec![Self::d("sampleTime", "Sample Time", 1.0)],
            SignalMathFcn => vec![Self::s("function", "Function", "sqrt")],
            SignalTrigFcn => vec![Self::s("function", "Function", "sin")],
            SignalDeadZone => vec![
                Self::d("lowerLimit", "Start of dead zone", -0.5),
                Self::d("upperLimit", "End of dead zone", 0.5),
            ],
            SignalRelop => vec![Self::s("op", "Operator", "<")],
            SignalLogical => vec![Self::s("op", "Operator", "AND")],
            SignalCompareToZero => vec![Self::s("op", "Operator", ">")],
            SignalBusCreator => vec![Self::s("field_names", "Field Names", "")],
            SignalBusSelector => vec![Self::s("field", "Field", "")],
            SignalReshape => vec![
                Self::d("rows", "Rows", 1.0),
                Self::d("cols", "Cols", 1.0),
                Self::s("shape", "Shape (alt form)", ""),
            ],
            SignalMatlabFcn => vec![
                Self::s("expression", "Expression", "u"),
                Self::s("function_body", "Function Body", ""),
            ],
            _ => vec![],
        }
    }
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
        assert!(sig.len() >= 4, "expected several signal categories, got {}", sig.len());
        assert_eq!(sig[0].0, NodeCategory::SignalSources);
        let total: usize = sig.iter().map(|(_, ks)| ks.len()).sum();
        assert!(total > 6, "library should list more than the curated palette ({total})");
        for (cat, kinds) in &sig {
            assert!(cat.is_signal_flow());
            assert!(!kinds.is_empty());
            assert!(kinds.iter().all(|k| !matches!(k, NodeKind::Start | NodeKind::End)));
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
        assert_eq!(NodeCategory::signal_flow_order().len(), 8);
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
            ControlFlow, Data, Io, Functions, Matrix, Other,
            SignalSources, SignalSinks, SignalContinuous, SignalDiscrete,
            SignalMath, SignalRouting, SignalLookup, SignalComposite,
            ChartStates, ChartJunctions, ChartFunctions,
        ];
        for c in all {
            assert!(!c.label().is_empty(), "{c:?} has no label");
            let _ = c.accent(); // every arm returns a color
            // Signal/state predicates partition the dialect-specific categories.
            assert!(!(c.is_signal_flow() && c.is_state_chart()), "{c:?} in two dialects");
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
    }

    #[test]
    fn param_fields_empty_for_parameterless_blocks() {
        assert!(SignalFlowParamSpec::fields(NodeKind::SignalAbs).is_empty());
        assert!(SignalFlowParamSpec::fields(NodeKind::SignalTerminator).is_empty());
        assert!(SignalFlowParamSpec::fields(NodeKind::Assignment).is_empty());
    }
}
