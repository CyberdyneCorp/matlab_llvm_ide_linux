//! Flowchart nodes: the `NodeKind` taxonomy (control-flow, signal-flow, and
//! state-chart blocks), the flat `NodeData` field-bag, ports, UI geometry, and
//! the per-kind derived data (category, shape, default ports, anchors). Ported
//! from `FlowchartModels.swift`; JSON keys match the schema 1:1.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::palette::NodeCategory;

/// One block in a flow. Only `id`/`kind` are strictly required on disk; the
/// rest carry schema defaults filled in by the codec.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowNode {
    pub id: String,
    pub kind: NodeKind,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub ports: FlowPorts,
    #[serde(default)]
    pub data: NodeData,
    #[serde(default)]
    pub ui: FlowUi,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent: Option<String>,
}

impl FlowNode {
    pub fn new(
        id: &str,
        kind: NodeKind,
        label: &str,
        ports: FlowPorts,
        data: NodeData,
        ui: FlowUi,
    ) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind,
            label: label.to_string(),
            ports,
            data,
            ui,
            parent: None,
        }
    }
}

/// Block kinds defined by schema §6. Serde raw values match the JSON `kind`
/// field exactly (renamed where the snake_case differs from the Rust name).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    // 6.1 Structural
    #[serde(rename = "start")]
    Start,
    #[serde(rename = "end")]
    End,
    #[serde(rename = "comment")]
    Comment,
    // 6.2 Linear statements
    #[serde(rename = "constant")]
    Constant,
    #[serde(rename = "variable")]
    Variable,
    #[serde(rename = "assignment")]
    Assignment,
    #[serde(rename = "expression")]
    Expression,
    #[serde(rename = "input")]
    Input,
    #[serde(rename = "display")]
    Display,
    #[serde(rename = "function_call")]
    FunctionCall,
    #[serde(rename = "matrix_literal")]
    MatrixLiteral,
    // 6.3 Control flow
    #[serde(rename = "if")]
    IfBlock,
    #[serde(rename = "for")]
    ForLoop,
    #[serde(rename = "while")]
    WhileLoop,
    #[serde(rename = "break")]
    BreakBlock,
    #[serde(rename = "continue")]
    ContinueBlock,
    #[serde(rename = "return")]
    ReturnBlock,
    // 6.4 Multi-flow
    #[serde(rename = "function_definition")]
    FunctionDefinition,
    #[serde(rename = "subflow_call")]
    SubflowCall,
    // 6.5 Custom
    #[serde(rename = "custom")]
    Custom,
    // 6.6 Signal-flow — sources
    #[serde(rename = "signal_constant")]
    SignalConstant,
    #[serde(rename = "signal_step")]
    SignalStep,
    #[serde(rename = "signal_sine")]
    SignalSine,
    #[serde(rename = "signal_pulse")]
    SignalPulse,
    #[serde(rename = "signal_ramp")]
    SignalRamp,
    #[serde(rename = "signal_clock")]
    SignalClock,
    #[serde(rename = "signal_chirp")]
    SignalChirp,
    #[serde(rename = "signal_noise")]
    SignalNoise,
    #[serde(rename = "signal_function_call_generator")]
    SignalFunctionCallGenerator,
    // sinks
    #[serde(rename = "signal_scope")]
    SignalScope,
    #[serde(rename = "signal_display")]
    SignalDisplay,
    #[serde(rename = "signal_to_workspace")]
    SignalToWorkspace,
    #[serde(rename = "signal_terminator")]
    SignalTerminator,
    // continuous
    #[serde(rename = "signal_integrator")]
    SignalIntegrator,
    #[serde(rename = "signal_derivative")]
    SignalDerivative,
    #[serde(rename = "signal_transfer_fcn")]
    SignalTransferFcn,
    #[serde(rename = "signal_state_space")]
    SignalStateSpace,
    #[serde(rename = "signal_zero_pole")]
    SignalZeroPole,
    #[serde(rename = "signal_transport_delay")]
    SignalTransportDelay,
    // discrete
    #[serde(rename = "signal_unit_delay")]
    SignalUnitDelay,
    #[serde(rename = "signal_zoh")]
    SignalZoh,
    #[serde(rename = "signal_discrete_integrator")]
    SignalDiscreteIntegrator,
    #[serde(rename = "signal_discrete_filter")]
    SignalDiscreteFilter,
    #[serde(rename = "signal_rate_transition")]
    SignalRateTransition,
    // math
    #[serde(rename = "signal_gain")]
    SignalGain,
    #[serde(rename = "signal_sum")]
    SignalSum,
    #[serde(rename = "signal_product")]
    SignalProduct,
    #[serde(rename = "signal_abs")]
    SignalAbs,
    #[serde(rename = "signal_saturation")]
    SignalSaturation,
    #[serde(rename = "signal_math_fcn")]
    SignalMathFcn,
    #[serde(rename = "signal_trig_fcn")]
    SignalTrigFcn,
    #[serde(rename = "signal_dead_zone")]
    SignalDeadZone,
    #[serde(rename = "signal_relop")]
    SignalRelop,
    #[serde(rename = "signal_logical")]
    SignalLogical,
    #[serde(rename = "signal_compare_to_zero")]
    SignalCompareToZero,
    #[serde(rename = "signal_compare_to_constant")]
    SignalCompareToConstant,
    #[serde(rename = "signal_relay")]
    SignalRelay,
    // routing
    #[serde(rename = "signal_mux")]
    SignalMux,
    #[serde(rename = "signal_demux")]
    SignalDemux,
    #[serde(rename = "signal_switch")]
    SignalSwitch,
    #[serde(rename = "signal_multiport_switch")]
    SignalMultiportSwitch,
    #[serde(rename = "signal_merge")]
    SignalMerge,
    #[serde(rename = "signal_goto")]
    SignalGoto,
    #[serde(rename = "signal_from")]
    SignalFrom,
    #[serde(rename = "signal_bus_creator")]
    SignalBusCreator,
    #[serde(rename = "signal_bus_selector")]
    SignalBusSelector,
    #[serde(rename = "signal_reshape")]
    SignalReshape,
    #[serde(rename = "signal_matlab_fcn")]
    SignalMatlabFcn,
    // lookup
    #[serde(rename = "signal_lookup_1d")]
    SignalLookup1D,
    #[serde(rename = "signal_lookup_2d")]
    SignalLookup2D,
    // composite
    #[serde(rename = "signal_subsystem")]
    SignalSubsystem,
    #[serde(rename = "signal_inport")]
    SignalInport,
    #[serde(rename = "signal_outport")]
    SignalOutport,
    #[serde(rename = "signal_enabled_subsystem")]
    SignalEnabledSubsystem,
    #[serde(rename = "signal_triggered_subsystem")]
    SignalTriggeredSubsystem,
    // 6.7 State-chart
    #[serde(rename = "state")]
    State,
    #[serde(rename = "junction_connective")]
    JunctionConnective,
    #[serde(rename = "junction_history")]
    JunctionHistory,
    #[serde(rename = "junction_entry")]
    JunctionEntry,
    #[serde(rename = "junction_exit")]
    JunctionExit,
    #[serde(rename = "junction_default")]
    JunctionDefault,
    #[serde(rename = "chart_fn_graphical")]
    ChartFnGraphical,
    #[serde(rename = "chart_fn_matlab")]
    ChartFnMatlab,
    #[serde(rename = "chart_fn_truth_table")]
    ChartFnTruthTable,
}

/// Geometric shape used for the node body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeShape {
    Rectangle,
    RoundedRect,
    Ellipse,
    Diamond,
    Parallelogram,
    Hexagon,
}

/// Which face of a node body a port lives on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PortAnchor {
    Top,
    Bottom,
    Left,
    Right,
}

impl NodeKind {
    /// Every kind, for palette enumeration + exhaustive tests.
    pub const ALL: [NodeKind; 84] = [
        NodeKind::Start, NodeKind::End, NodeKind::Comment, NodeKind::Constant,
        NodeKind::Variable, NodeKind::Assignment, NodeKind::Expression, NodeKind::Input,
        NodeKind::Display, NodeKind::FunctionCall, NodeKind::MatrixLiteral, NodeKind::IfBlock,
        NodeKind::ForLoop, NodeKind::WhileLoop, NodeKind::BreakBlock, NodeKind::ContinueBlock,
        NodeKind::ReturnBlock, NodeKind::FunctionDefinition, NodeKind::SubflowCall, NodeKind::Custom,
        NodeKind::SignalConstant, NodeKind::SignalStep, NodeKind::SignalSine, NodeKind::SignalPulse,
        NodeKind::SignalRamp, NodeKind::SignalClock, NodeKind::SignalChirp, NodeKind::SignalNoise,
        NodeKind::SignalFunctionCallGenerator, NodeKind::SignalScope, NodeKind::SignalDisplay,
        NodeKind::SignalToWorkspace, NodeKind::SignalTerminator, NodeKind::SignalIntegrator,
        NodeKind::SignalDerivative, NodeKind::SignalTransferFcn, NodeKind::SignalStateSpace,
        NodeKind::SignalZeroPole, NodeKind::SignalTransportDelay, NodeKind::SignalUnitDelay,
        NodeKind::SignalZoh, NodeKind::SignalDiscreteIntegrator, NodeKind::SignalDiscreteFilter,
        NodeKind::SignalRateTransition, NodeKind::SignalGain, NodeKind::SignalSum,
        NodeKind::SignalProduct, NodeKind::SignalAbs, NodeKind::SignalSaturation,
        NodeKind::SignalMathFcn, NodeKind::SignalTrigFcn, NodeKind::SignalDeadZone,
        NodeKind::SignalRelop, NodeKind::SignalLogical, NodeKind::SignalCompareToZero,
        NodeKind::SignalCompareToConstant, NodeKind::SignalRelay, NodeKind::SignalMux,
        NodeKind::SignalDemux, NodeKind::SignalSwitch, NodeKind::SignalMultiportSwitch,
        NodeKind::SignalMerge, NodeKind::SignalGoto, NodeKind::SignalFrom,
        NodeKind::SignalBusCreator, NodeKind::SignalBusSelector, NodeKind::SignalReshape,
        NodeKind::SignalMatlabFcn, NodeKind::SignalLookup1D, NodeKind::SignalLookup2D,
        NodeKind::SignalSubsystem, NodeKind::SignalInport, NodeKind::SignalOutport,
        NodeKind::SignalEnabledSubsystem, NodeKind::SignalTriggeredSubsystem, NodeKind::State,
        NodeKind::JunctionConnective, NodeKind::JunctionHistory, NodeKind::JunctionEntry,
        NodeKind::JunctionExit, NodeKind::JunctionDefault, NodeKind::ChartFnGraphical,
        NodeKind::ChartFnMatlab, NodeKind::ChartFnTruthTable,
    ];

    /// Palette / inspector category.
    pub fn category(self) -> NodeCategory {
        use NodeCategory as C;
        use NodeKind::*;
        match self {
            Start | End | Comment => C::Other,
            IfBlock | ForLoop | WhileLoop | BreakBlock | ContinueBlock | ReturnBlock => {
                C::ControlFlow
            }
            Input | Display => C::Io,
            Constant | Variable | Assignment | Expression => C::Data,
            FunctionDefinition | FunctionCall | SubflowCall | Custom => C::Functions,
            MatrixLiteral => C::Matrix,
            SignalConstant | SignalStep | SignalSine | SignalPulse | SignalRamp | SignalClock
            | SignalChirp | SignalNoise | SignalFunctionCallGenerator => C::SignalSources,
            SignalScope | SignalDisplay | SignalToWorkspace | SignalTerminator => C::SignalSinks,
            SignalIntegrator | SignalDerivative | SignalTransferFcn | SignalStateSpace
            | SignalZeroPole | SignalTransportDelay => C::SignalContinuous,
            SignalUnitDelay | SignalZoh | SignalDiscreteIntegrator | SignalDiscreteFilter
            | SignalRateTransition => C::SignalDiscrete,
            SignalGain | SignalSum | SignalProduct | SignalAbs | SignalSaturation | SignalMathFcn
            | SignalTrigFcn | SignalDeadZone | SignalRelop | SignalLogical | SignalCompareToZero
            | SignalCompareToConstant | SignalRelay | SignalMatlabFcn => C::SignalMath,
            SignalMux | SignalDemux | SignalSwitch | SignalMultiportSwitch | SignalMerge
            | SignalGoto | SignalFrom | SignalBusCreator | SignalBusSelector | SignalReshape => {
                C::SignalRouting
            }
            SignalLookup1D | SignalLookup2D => C::SignalLookup,
            SignalSubsystem | SignalInport | SignalOutport | SignalEnabledSubsystem
            | SignalTriggeredSubsystem => C::SignalComposite,
            State => C::ChartStates,
            JunctionConnective | JunctionHistory | JunctionEntry | JunctionExit
            | JunctionDefault => C::ChartJunctions,
            ChartFnGraphical | ChartFnMatlab | ChartFnTruthTable => C::ChartFunctions,
        }
    }

    pub fn is_signal_flow(self) -> bool {
        self.category().is_signal_flow()
    }

    pub fn is_state_chart(self) -> bool {
        self.category().is_state_chart()
    }

    /// Geometric shape used for the node body.
    pub fn shape(self) -> NodeShape {
        use NodeKind::*;
        match self {
            Start | End => NodeShape::Ellipse,
            IfBlock => NodeShape::Diamond,
            Input | Display => NodeShape::Parallelogram,
            ForLoop | WhileLoop => NodeShape::Hexagon,
            BreakBlock | ContinueBlock | ReturnBlock | Comment | FunctionDefinition => {
                NodeShape::RoundedRect
            }
            State => NodeShape::RoundedRect,
            JunctionConnective | JunctionHistory | JunctionEntry | JunctionExit
            | JunctionDefault => NodeShape::Ellipse,
            ChartFnGraphical | ChartFnMatlab | ChartFnTruthTable => NodeShape::Rectangle,
            _ => NodeShape::Rectangle,
        }
    }

    /// Display label shown in palette buttons and the inspector header.
    pub fn display_name(self) -> &'static str {
        use NodeKind::*;
        match self {
            Start => "Start",
            End => "End",
            Comment => "Comment",
            Constant => "Constant",
            Variable => "Variable",
            Assignment => "Assignment",
            Expression => "Expression",
            Input => "Input",
            Display => "Display",
            IfBlock => "If / Else",
            ForLoop => "For Loop",
            WhileLoop => "While Loop",
            BreakBlock => "Break",
            ContinueBlock => "Continue",
            ReturnBlock => "Return",
            FunctionDefinition => "Function Definition",
            FunctionCall => "Function Call",
            SubflowCall => "Subflow Call",
            MatrixLiteral => "Matrix Literal",
            Custom => "Custom Block",
            SignalConstant => "Constant",
            SignalStep => "Step",
            SignalSine => "Sine Wave",
            SignalPulse => "Pulse Generator",
            SignalRamp => "Ramp",
            SignalScope => "Scope",
            SignalDisplay => "Display",
            SignalToWorkspace => "To Workspace",
            SignalTerminator => "Terminator",
            SignalIntegrator => "Integrator",
            SignalDerivative => "Derivative",
            SignalTransferFcn => "Transfer Fcn",
            SignalStateSpace => "State-Space",
            SignalUnitDelay => "Unit Delay",
            SignalZoh => "Zero-Order Hold",
            SignalGain => "Gain",
            SignalSum => "Sum",
            SignalProduct => "Product",
            SignalAbs => "Abs",
            SignalSaturation => "Saturation",
            SignalMux => "Mux",
            SignalDemux => "Demux",
            SignalSwitch => "Switch",
            SignalSubsystem => "Subsystem",
            SignalInport => "In",
            SignalOutport => "Out",
            SignalEnabledSubsystem => "Enabled Subsystem",
            SignalTriggeredSubsystem => "Triggered Subsystem",
            SignalClock => "Clock",
            SignalChirp => "Chirp",
            SignalNoise => "Random Noise",
            SignalFunctionCallGenerator => "Function-Call Generator",
            SignalZeroPole => "Zero-Pole",
            SignalTransportDelay => "Transport Delay",
            SignalDiscreteIntegrator => "Discrete Integrator",
            SignalDiscreteFilter => "Discrete Filter",
            SignalRateTransition => "Rate Transition",
            SignalMathFcn => "Math Function",
            SignalTrigFcn => "Trig Function",
            SignalDeadZone => "Dead Zone",
            SignalRelop => "Relational Op",
            SignalLogical => "Logical Op",
            SignalCompareToZero => "Compare to Zero",
            SignalCompareToConstant => "Compare to Constant",
            SignalRelay => "Relay",
            SignalMultiportSwitch => "Multiport Switch",
            SignalMerge => "Merge",
            SignalLookup1D => "1-D Lookup Table",
            SignalLookup2D => "2-D Lookup Table",
            SignalGoto => "Goto",
            SignalFrom => "From",
            SignalMatlabFcn => "MATLAB Function",
            SignalBusCreator => "Bus Creator",
            SignalBusSelector => "Bus Selector",
            SignalReshape => "Reshape",
            State => "State",
            JunctionConnective => "Junction",
            JunctionHistory => "History Junction",
            JunctionEntry => "Entry Junction",
            JunctionExit => "Exit Junction",
            JunctionDefault => "Default Transition",
            ChartFnGraphical => "Graphical Function",
            ChartFnMatlab => "MATLAB Function",
            ChartFnTruthTable => "Truth Table",
        }
    }

    /// Whether matlabc can usefully pause on this kind (lowers to ≥1 statement).
    pub fn is_executable(self) -> bool {
        use NodeKind::*;
        !matches!(
            self,
            Start | End
                | Comment
                | FunctionDefinition
                | JunctionConnective
                | JunctionHistory
                | JunctionEntry
                | JunctionExit
                | JunctionDefault
        )
    }

    /// Whether the "Comment Through" action applies (simple linear blocks only).
    pub fn can_be_commented_through(self) -> bool {
        use NodeKind::*;
        matches!(
            self,
            Variable
                | Constant
                | Assignment
                | Expression
                | Display
                | Input
                | FunctionCall
                | SubflowCall
                | Custom
                | MatrixLiteral
        )
    }

    /// True when this `signal_*` block interrupts an algebraic loop (carries
    /// state across a tick). Always false for control-flow kinds.
    pub fn breaks_algebraic_loop(self) -> bool {
        use NodeKind::*;
        matches!(
            self,
            SignalIntegrator
                | SignalUnitDelay
                | SignalZoh
                | SignalDiscreteIntegrator
                | SignalDiscreteFilter
                | SignalRateTransition
                | SignalTransportDelay
        )
    }

    /// Default body size for this kind.
    pub fn default_size(self) -> FlowSize {
        use NodeKind::*;
        if self.is_state_chart() {
            return match self {
                State => FlowSize { width: 160.0, height: 96.0 },
                ChartFnGraphical | ChartFnMatlab | ChartFnTruthTable => {
                    FlowSize { width: 150.0, height: 72.0 }
                }
                _ => FlowSize { width: 28.0, height: 28.0 },
            };
        }
        if self.is_signal_flow() {
            return match self {
                SignalSubsystem | SignalEnabledSubsystem | SignalTriggeredSubsystem => {
                    FlowSize { width: 150.0, height: 80.0 }
                }
                SignalInport | SignalOutport => FlowSize { width: 90.0, height: 44.0 },
                _ => FlowSize { width: 120.0, height: 56.0 },
            };
        }
        match self.shape() {
            NodeShape::Diamond | NodeShape::Hexagon => FlowSize { width: 220.0, height: 80.0 },
            _ => FlowSize { width: 180.0, height: 56.0 },
        }
    }

    /// Which side of the node body a given port anchors to. `None` for ports
    /// not declared on this kind's default lineup.
    pub fn port_anchor(self, id: &str) -> Option<PortAnchor> {
        use NodeKind::*;
        use PortAnchor::*;
        if self.is_signal_flow() {
            return self.signal_flow_port_anchor(id);
        }
        if self.is_state_chart() {
            return match id {
                "in" => Some(Left),
                "out" => Some(Right),
                _ => None,
            };
        }
        match self {
            Start => (id == "out").then_some(Bottom),
            End => (id == "in").then_some(Top),
            IfBlock => match id {
                "in" => Some(Top),
                "true" => Some(Bottom),
                "false" => Some(Right),
                _ => None,
            },
            ForLoop | WhileLoop => match id {
                "in" => Some(Top),
                "body" => Some(Bottom),
                "done" => Some(Right),
                _ => None,
            },
            _ => match id {
                "in" => Some(Top),
                "out" => Some(Bottom),
                _ => None,
            },
        }
    }

    fn signal_flow_port_anchor(self, id: &str) -> Option<PortAnchor> {
        use NodeKind::*;
        use PortAnchor::*;
        match self {
            SignalConstant | SignalStep | SignalSine | SignalPulse | SignalRamp | SignalInport
            | SignalClock | SignalChirp | SignalNoise | SignalFunctionCallGenerator
            | SignalFrom => (id == "out").then_some(Right),
            SignalScope | SignalDisplay | SignalToWorkspace | SignalTerminator | SignalOutport
            | SignalGoto => (id == "in").then_some(Left),
            SignalSubsystem | SignalEnabledSubsystem | SignalTriggeredSubsystem => {
                if id.starts_with("out") {
                    Some(Right)
                } else if id.starts_with("in") {
                    Some(Left)
                } else {
                    None
                }
            }
            SignalDemux => {
                if id == "in" {
                    Some(Left)
                } else if id.starts_with("out") {
                    Some(Right)
                } else {
                    None
                }
            }
            SignalMux => {
                if id == "out" {
                    Some(Right)
                } else if id.starts_with("in") {
                    Some(Left)
                } else {
                    None
                }
            }
            SignalSwitch => match id {
                "in1" | "in2" | "ctrl" => Some(Left),
                "out" => Some(Right),
                _ => None,
            },
            SignalSum | SignalProduct | SignalRelop | SignalLogical | SignalMultiportSwitch
            | SignalMerge | SignalLookup2D | SignalBusCreator => {
                if id == "out" {
                    Some(Right)
                } else if id.starts_with("in") {
                    Some(Left)
                } else {
                    None
                }
            }
            SignalIntegrator => match id {
                "in" | "reset" | "init" => Some(Left),
                "out" => Some(Right),
                _ => None,
            },
            _ => match id {
                "in" => Some(Left),
                "out" => Some(Right),
                _ => None,
            },
        }
    }

    /// Default in/out port lineup per schema §5.
    pub fn default_ports(self) -> FlowPorts {
        use NodeKind::*;
        let p = FlowPort::new;
        match self {
            Start => FlowPorts { inputs: vec![], outputs: vec![p("out")] },
            End => FlowPorts { inputs: vec![p("in")], outputs: vec![] },
            IfBlock => FlowPorts {
                inputs: vec![p("in")],
                outputs: vec![p("true"), p("false")],
            },
            ForLoop | WhileLoop => FlowPorts {
                inputs: vec![p("in")],
                outputs: vec![p("body"), p("done")],
            },
            SignalConstant | SignalStep | SignalSine | SignalPulse | SignalRamp | SignalInport
            | SignalClock | SignalChirp | SignalNoise | SignalFunctionCallGenerator
            | SignalFrom => FlowPorts { inputs: vec![], outputs: vec![p("out")] },
            SignalScope | SignalDisplay | SignalToWorkspace | SignalTerminator | SignalOutport
            | SignalGoto => FlowPorts { inputs: vec![p("in")], outputs: vec![] },
            SignalSubsystem | SignalEnabledSubsystem | SignalTriggeredSubsystem => {
                FlowPorts { inputs: vec![p("in1")], outputs: vec![p("out1")] }
            }
            SignalSum | SignalProduct | SignalMux | SignalRelop | SignalLogical
            | SignalBusCreator | SignalLookup2D => {
                FlowPorts { inputs: vec![p("in1"), p("in2")], outputs: vec![p("out")] }
            }
            SignalDemux => FlowPorts {
                inputs: vec![p("in")],
                outputs: vec![p("out1"), p("out2")],
            },
            SignalSwitch => FlowPorts {
                inputs: vec![p("in1"), p("ctrl"), p("in2")],
                outputs: vec![p("out")],
            },
            SignalMultiportSwitch => FlowPorts {
                inputs: vec![p("in1"), p("in2"), p("in3")],
                outputs: vec![p("out")],
            },
            SignalMerge => FlowPorts { inputs: vec![p("in1"), p("in2")], outputs: vec![p("out")] },
            _ => FlowPorts { inputs: vec![p("in")], outputs: vec![p("out")] },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlowPort {
    pub id: String,
}

impl FlowPort {
    pub fn new(id: &str) -> FlowPort {
        FlowPort { id: id.to_string() }
    }
}

/// JSON uses `in`/`out` as keys — renamed to dodge Rust's reserved word.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FlowPorts {
    #[serde(rename = "in", default)]
    pub inputs: Vec<FlowPort>,
    #[serde(rename = "out", default)]
    pub outputs: Vec<FlowPort>,
}

/// Union of fields used across every node kind, per schema §6. Only the subset
/// relevant to a given `NodeKind` is read; the rest stays `None` and is omitted.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct NodeData {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub lhs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rhs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub callee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub args: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cond: Option<String>,
    #[serde(rename = "var", skip_serializing_if = "Option::is_none", default)]
    pub loop_var: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub iter: Option<String>,
    #[serde(rename = "flow_id", skip_serializing_if = "Option::is_none", default)]
    pub flow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rows: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub path: Option<String>,
    #[serde(rename = "library_id", skip_serializing_if = "Option::is_none", default)]
    pub library_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub inputs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outputs: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<BTreeMap<String, ParamValue>>,
    #[serde(rename = "sample_time", skip_serializing_if = "Option::is_none", default)]
    pub sample_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub units: Option<String>,
    #[serde(rename = "data_type", skip_serializing_if = "Option::is_none", default)]
    pub data_type: Option<String>,
    #[serde(rename = "log_signal", skip_serializing_if = "Option::is_none", default)]
    pub log_signal: Option<bool>,
    #[serde(rename = "enable_block", skip_serializing_if = "Option::is_none", default)]
    pub enable_block: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tag: Option<String>,
    #[serde(rename = "mask_params", skip_serializing_if = "Option::is_none", default)]
    pub mask_params: Option<BTreeMap<String, String>>,
    // mStateflow action bodies — camelCase JSON keys to match shipped fixtures.
    #[serde(rename = "entryAction", skip_serializing_if = "Option::is_none", default)]
    pub entry_action: Option<String>,
    #[serde(rename = "duringAction", skip_serializing_if = "Option::is_none", default)]
    pub during_action: Option<String>,
    #[serde(rename = "exitAction", skip_serializing_if = "Option::is_none", default)]
    pub exit_action: Option<String>,
    #[serde(rename = "onEventActions", skip_serializing_if = "Option::is_none", default)]
    pub on_event_actions: Option<BTreeMap<String, String>>,
}

/// JSON-flavored scalar union for signal-flow block parameters. Encodes as the
/// bare scalar (no `{type,value}` wrapping).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Double(f64),
    Bool(bool),
    Str(String),
}

impl ParamValue {
    /// Text form for the inspector. Doubles with no fractional part render as
    /// integers (`2.0` → "2").
    pub fn display_string(&self) -> String {
        match self {
            ParamValue::Double(d) => {
                if *d == d.round() && d.abs() < 1e15 {
                    format!("{}", *d as i64)
                } else {
                    format!("{d}")
                }
            }
            ParamValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            ParamValue::Str(s) => s.clone(),
        }
    }

    /// Parse a user-typed string into a typed value; falls back to `Str`.
    pub fn parse(input: &str) -> ParamValue {
        let trimmed = input.trim();
        match trimmed.to_lowercase().as_str() {
            "true" => return ParamValue::Bool(true),
            "false" => return ParamValue::Bool(false),
            _ => {}
        }
        if let Ok(d) = trimmed.parse::<f64>() {
            return ParamValue::Double(d);
        }
        ParamValue::Str(input.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlowUi {
    pub position: FlowPosition,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub size: Option<FlowSize>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub collapsed: Option<bool>,
}

impl FlowUi {
    pub fn at(position: FlowPosition) -> FlowUi {
        FlowUi { position, size: None, collapsed: None }
    }
}

impl Default for FlowUi {
    fn default() -> Self {
        FlowUi::at(FlowPosition { x: 0.0, y: 0.0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_kind_serde_roundtrip_for_renamed_keys() {
        let cases = [
            (NodeKind::IfBlock, "\"if\""),
            (NodeKind::ForLoop, "\"for\""),
            (NodeKind::FunctionCall, "\"function_call\""),
            (NodeKind::SignalZoh, "\"signal_zoh\""),
            (NodeKind::SignalLookup1D, "\"signal_lookup_1d\""),
        ];
        for (kind, json) in cases {
            assert_eq!(serde_json::to_string(&kind).unwrap(), json);
            assert_eq!(serde_json::from_str::<NodeKind>(json).unwrap(), kind);
        }
    }

    #[test]
    fn category_partitions_dialects() {
        assert_eq!(NodeKind::IfBlock.category(), NodeCategory::ControlFlow);
        assert!(NodeKind::SignalGain.is_signal_flow());
        assert!(NodeKind::State.is_state_chart());
        assert!(!NodeKind::Start.is_signal_flow());
    }

    #[test]
    fn shapes_match_reference() {
        assert_eq!(NodeKind::Start.shape(), NodeShape::Ellipse);
        assert_eq!(NodeKind::IfBlock.shape(), NodeShape::Diamond);
        assert_eq!(NodeKind::ForLoop.shape(), NodeShape::Hexagon);
        assert_eq!(NodeKind::Input.shape(), NodeShape::Parallelogram);
        assert_eq!(NodeKind::Assignment.shape(), NodeShape::Rectangle);
    }

    #[test]
    fn executable_excludes_structural_and_junctions() {
        assert!(!NodeKind::Start.is_executable());
        assert!(!NodeKind::Comment.is_executable());
        assert!(!NodeKind::JunctionEntry.is_executable());
        assert!(NodeKind::Assignment.is_executable());
    }

    #[test]
    fn default_ports_for_control_flow() {
        let p = NodeKind::IfBlock.default_ports();
        assert_eq!(p.inputs.len(), 1);
        assert_eq!(p.outputs.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["true", "false"]);
        let start = NodeKind::Start.default_ports();
        assert!(start.inputs.is_empty());
        assert_eq!(start.outputs[0].id, "out");
    }

    #[test]
    fn default_ports_for_signal_blocks() {
        assert!(NodeKind::SignalConstant.default_ports().inputs.is_empty());
        assert_eq!(NodeKind::SignalSum.default_ports().inputs.len(), 2);
        assert_eq!(NodeKind::SignalSwitch.default_ports().inputs.len(), 3);
    }

    #[test]
    fn port_anchor_conventions() {
        assert_eq!(NodeKind::Start.port_anchor("out"), Some(PortAnchor::Bottom));
        assert_eq!(NodeKind::IfBlock.port_anchor("false"), Some(PortAnchor::Right));
        assert_eq!(NodeKind::SignalGain.port_anchor("in"), Some(PortAnchor::Left));
        assert_eq!(NodeKind::SignalGain.port_anchor("out"), Some(PortAnchor::Right));
        assert_eq!(NodeKind::Assignment.port_anchor("nope"), None);
    }

    #[test]
    fn default_size_varies_by_family() {
        assert_eq!(NodeKind::IfBlock.default_size().width, 220.0);
        assert_eq!(NodeKind::SignalGain.default_size().width, 120.0);
        assert_eq!(NodeKind::State.default_size().width, 160.0);
        assert_eq!(NodeKind::Assignment.default_size().width, 180.0);
    }

    #[test]
    fn breaks_algebraic_loop_only_for_stateful_blocks() {
        assert!(NodeKind::SignalIntegrator.breaks_algebraic_loop());
        assert!(NodeKind::SignalUnitDelay.breaks_algebraic_loop());
        assert!(!NodeKind::SignalGain.breaks_algebraic_loop());
        assert!(!NodeKind::Assignment.breaks_algebraic_loop());
    }

    #[test]
    fn param_value_untagged_serde() {
        assert_eq!(serde_json::to_string(&ParamValue::Double(2.0)).unwrap(), "2.0");
        assert_eq!(serde_json::to_string(&ParamValue::Bool(true)).unwrap(), "true");
        assert_eq!(serde_json::to_string(&ParamValue::Str("hi".into())).unwrap(), "\"hi\"");
        assert_eq!(serde_json::from_str::<ParamValue>("3.5").unwrap(), ParamValue::Double(3.5));
        assert_eq!(serde_json::from_str::<ParamValue>("true").unwrap(), ParamValue::Bool(true));
        assert_eq!(
            serde_json::from_str::<ParamValue>("\"x\"").unwrap(),
            ParamValue::Str("x".into())
        );
    }

    #[test]
    fn param_value_display_and_parse() {
        assert_eq!(ParamValue::Double(2.0).display_string(), "2");
        assert_eq!(ParamValue::Double(0.001).display_string(), "0.001");
        assert_eq!(ParamValue::parse("  true "), ParamValue::Bool(true));
        assert_eq!(ParamValue::parse("4"), ParamValue::Double(4.0));
        assert_eq!(ParamValue::parse("+-+"), ParamValue::Str("+-+".into()));
    }

    #[test]
    fn node_data_renames_var_and_flow_id() {
        let data = NodeData {
            loop_var: Some("i".into()),
            flow_id: Some("flow_x".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("\"var\":\"i\""));
        assert!(json.contains("\"flow_id\":\"flow_x\""));
        assert!(!json.contains("loop_var"));
    }

    #[test]
    fn all_kinds_have_unique_display_serialization() {
        // Sanity: every variant in ALL serializes distinctly.
        let mut seen = std::collections::HashSet::new();
        for k in NodeKind::ALL {
            let j = serde_json::to_string(&k).unwrap();
            assert!(seen.insert(j), "duplicate serialization for {k:?}");
        }
    }

    #[test]
    fn every_kind_exposes_consistent_derived_data() {
        // Exhaustively exercise the per-kind tables so a missing/wrong arm is
        // caught and the giant match expressions are fully covered.
        for k in NodeKind::ALL {
            assert!(!k.display_name().is_empty(), "{k:?} has empty name");
            let _ = k.category();
            let _ = k.shape();
            let _ = k.is_executable();
            let _ = k.can_be_commented_through();
            let _ = k.breaks_algebraic_loop();
            let size = k.default_size();
            assert!(size.width > 0.0 && size.height > 0.0, "{k:?} bad size");
            let ports = k.default_ports();
            // Every declared port resolves to an anchor.
            for p in ports.inputs.iter().chain(ports.outputs.iter()) {
                assert!(k.port_anchor(&p.id).is_some(), "{k:?} port {} has no anchor", p.id);
            }
            // Dialect flags are consistent with the category.
            assert_eq!(k.is_signal_flow(), k.category().is_signal_flow());
            assert_eq!(k.is_state_chart(), k.category().is_state_chart());
        }
    }

    #[test]
    fn param_specs_resolve_for_every_kind() {
        use super::super::palette::SignalFlowParamSpec;
        for k in NodeKind::ALL {
            // Must not panic; many return [] which is fine.
            let _ = SignalFlowParamSpec::fields(k);
        }
    }
}
