//! Debug (DAP) model types: function/exception/data breakpoints and the
//! stack-frame / variable / evaluation rows. Mirrors `Models.swift`.

use super::ids::next_id;

/// Breakpoint that fires on entry to the named function.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionBreakpoint {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub condition: Option<String>,
    pub hit_condition: Option<String>,
    pub log_message: Option<String>,
    pub dap_id: Option<i64>,
    pub verified: bool,
    pub line: Option<usize>,
}

impl FunctionBreakpoint {
    pub fn new(name: impl Into<String>) -> FunctionBreakpoint {
        FunctionBreakpoint {
            id: next_id(),
            name: name.into(),
            enabled: true,
            condition: None,
            hit_condition: None,
            log_message: None,
            dap_id: None,
            verified: false,
            line: None,
        }
    }
}

/// Exception filter advertised by the adapter and toggled per session.
#[derive(Clone, Debug, PartialEq)]
pub struct ExceptionFilter {
    pub filter: String,
    pub label: String,
    pub enabled: bool,
    pub condition: Option<String>,
    pub supports_condition: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DataAccess {
    Read,
    Write,
    ReadWrite,
}

impl DataAccess {
    pub const ALL: [DataAccess; 3] = [DataAccess::Read, DataAccess::Write, DataAccess::ReadWrite];

    /// DAP wire value.
    pub fn wire(self) -> &'static str {
        match self {
            DataAccess::Read => "read",
            DataAccess::Write => "write",
            DataAccess::ReadWrite => "readWrite",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            DataAccess::Read => "on read",
            DataAccess::Write => "on write",
            DataAccess::ReadWrite => "on read/write",
        }
    }
}

/// Data breakpoint — fires on read/write of a workspace name.
#[derive(Clone, Debug, PartialEq)]
pub struct DataBreakpoint {
    pub id: u64,
    pub name: String,
    pub access: DataAccess,
    pub enabled: bool,
    pub data_id: Option<String>,
    pub condition: Option<String>,
    pub hit_condition: Option<String>,
    pub dap_id: Option<i64>,
    pub verified: bool,
}

impl DataBreakpoint {
    pub fn new(name: impl Into<String>, access: DataAccess) -> DataBreakpoint {
        DataBreakpoint {
            id: next_id(),
            name: name.into(),
            access,
            enabled: true,
            data_id: None,
            condition: None,
            hit_condition: None,
            dap_id: None,
            verified: false,
        }
    }
}

/// One call-stack frame.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DapStackFrame {
    pub id: i64,
    pub name: String,
    pub source_path: Option<String>,
    pub line: Option<usize>,
}

/// One variable in the Debug panel's Locals / drill-in tree.
#[derive(Clone, Debug, PartialEq)]
pub struct DapVariable {
    pub name: String,
    pub value: String,
    pub type_hint: Option<String>,
    pub variables_reference: i64,
    pub indexed_variables: Option<i64>,
    pub named_variables: Option<i64>,
}

impl DapVariable {
    pub fn scalar(name: impl Into<String>, value: impl Into<String>) -> DapVariable {
        DapVariable {
            name: name.into(),
            value: value.into(),
            type_hint: None,
            variables_reference: 0,
            indexed_variables: None,
            named_variables: None,
        }
    }

    /// Compound types (matrices, structs, classes) can be drilled into.
    pub fn is_expandable(&self) -> bool {
        self.variables_reference != 0
    }
}

/// One row in the Watch-box history.
#[derive(Clone, Debug, PartialEq)]
pub struct DapEvaluation {
    pub id: u64,
    pub expression: String,
    pub result: String,
    pub type_hint: Option<String>,
}

impl DapEvaluation {
    pub fn new(expression: impl Into<String>, result: impl Into<String>) -> DapEvaluation {
        DapEvaluation {
            id: next_id(),
            expression: expression.into(),
            result: result.into(),
            type_hint: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_access_wire_values() {
        assert_eq!(DataAccess::ReadWrite.wire(), "readWrite");
        assert_eq!(DataAccess::Read.display_name(), "on read");
    }

    #[test]
    fn function_breakpoint_defaults() {
        let f = FunctionBreakpoint::new("foo");
        assert!(f.enabled);
        assert!(!f.verified);
        assert_eq!(f.name, "foo");
    }

    #[test]
    fn variable_expandable_on_nonzero_ref() {
        assert!(!DapVariable::scalar("x", "1").is_expandable());
        let mut v = DapVariable::scalar("m", "3x3 double");
        v.variables_reference = 9;
        assert!(v.is_expandable());
    }

    #[test]
    fn evaluation_carries_expression_and_result() {
        let e = DapEvaluation::new("x+1", "4");
        assert_eq!(e.expression, "x+1");
        assert_eq!(e.result, "4");
    }
}
