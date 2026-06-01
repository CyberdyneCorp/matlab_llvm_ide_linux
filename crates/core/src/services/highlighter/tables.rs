//! Per-language keyword / control-word / builtin tables, ported verbatim from
//! `SyntaxHighlighter.swift`. Each is a static `&[&str]` looked up via a
//! `HashSet` per highlight call.

pub(super) static MATLAB_KEYWORDS: &[&str] = &[
    "function", "end", "if", "elseif", "else", "for", "while", "do", "switch", "case",
    "otherwise", "try", "catch", "return", "break", "continue", "global", "persistent",
    "classdef", "properties", "methods", "events", "enumeration", "parfor",
];
pub(super) static MATLAB_CONTROL: &[&str] =
    &["true", "false", "nargin", "nargout", "varargin", "varargout"];
pub(super) static MATLAB_BUILTINS: &[&str] = &[
    "disp", "fprintf", "printf", "error", "warning", "assert", "length", "size", "numel",
    "zeros", "ones", "eye", "rand", "randn", "fft", "ifft", "fft2", "ifft2", "abs", "real",
    "imag", "complex", "exp", "log", "sqrt", "sin", "cos", "tan", "atan", "atan2", "sum",
    "prod", "mean", "min", "max", "sort", "find", "bitand", "bitor", "bitxor", "bitshift",
    "struct", "cell", "isempty", "isnumeric", "ischar", "mat2str", "whos", "who", "clear",
    "plot", "hold", "title", "xlabel", "ylabel", "grid", "linspace", "diag", "inv", "det",
    "eig", "mflowlink_run", "readmatrix", "readtable", "save", "load",
];

pub(super) static CPP_KEYWORDS: &[&str] = &[
    "alignas", "alignof", "and", "asm", "auto", "bool", "break", "case", "catch", "char",
    "char8_t", "char16_t", "char32_t", "class", "co_await", "co_return", "co_yield", "concept",
    "const", "consteval", "constexpr", "constinit", "const_cast", "continue", "decltype",
    "default", "delete", "do", "double", "dynamic_cast", "else", "enum", "explicit", "export",
    "extern", "float", "for", "friend", "goto", "if", "import", "inline", "int", "long",
    "module", "mutable", "namespace", "new", "noexcept", "not", "operator", "or", "private",
    "protected", "public", "register", "reinterpret_cast", "requires", "return", "short",
    "signed", "sizeof", "static", "static_assert", "static_cast", "struct", "switch", "template",
    "this", "thread_local", "throw", "try", "typedef", "typeid", "typename", "union", "unsigned",
    "using", "virtual", "void", "volatile", "wchar_t", "while", "xor", "include", "define",
    "undef", "ifdef", "ifndef", "endif", "pragma",
];
pub(super) static CPP_CONTROL: &[&str] = &["true", "false", "nullptr", "NULL", "this"];
pub(super) static CPP_BUILTINS: &[&str] = &[
    "std", "cout", "cin", "cerr", "endl", "string", "vector", "array", "map", "set",
    "unordered_map", "unordered_set", "shared_ptr", "unique_ptr", "weak_ptr", "make_shared",
    "make_unique", "move", "forward", "swap", "printf", "scanf", "fprintf", "fopen", "fclose",
    "malloc", "free", "memcpy", "memset", "strlen", "strcpy", "strcmp", "size_t", "ptrdiff_t",
    "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t",
    "Matrix", "VectorXd", "MatrixXd",
];

pub(super) static PYTHON_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda",
    "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield", "match",
    "case",
];
pub(super) static PYTHON_CONTROL: &[&str] = &["True", "False", "None", "self", "cls"];
pub(super) static PYTHON_BUILTINS: &[&str] = &[
    "print", "len", "range", "int", "float", "str", "list", "dict", "tuple", "set", "frozenset",
    "bytes", "bytearray", "bool", "map", "filter", "zip", "enumerate", "sum", "min", "max", "abs",
    "sorted", "reversed", "any", "all", "input", "open", "type", "isinstance", "issubclass",
    "hasattr", "getattr", "setattr", "delattr", "callable", "iter", "next", "format", "repr",
    "hash", "id", "help", "dir", "vars", "globals", "locals", "staticmethod", "classmethod",
    "property", "super", "object", "Exception", "BaseException", "ValueError", "TypeError",
    "KeyError", "IndexError", "NotImplementedError", "StopIteration", "RuntimeError", "np",
    "numpy", "linspace", "rand", "matforge",
];

pub(super) static TYPESCRIPT_KEYWORDS: &[&str] = &[
    "abstract", "any", "as", "async", "await", "boolean", "break", "case", "catch", "class",
    "const", "constructor", "continue", "debugger", "declare", "default", "delete", "do", "else",
    "enum", "export", "extends", "finally", "for", "from", "function", "get", "if", "implements",
    "import", "in", "infer", "instanceof", "interface", "is", "keyof", "let", "module",
    "namespace", "never", "new", "number", "object", "of", "package", "private", "protected",
    "public", "readonly", "require", "return", "set", "static", "string", "super", "switch",
    "symbol", "throw", "try", "type", "typeof", "undefined", "unique", "unknown", "var", "void",
    "while", "with", "yield",
];
pub(super) static TYPESCRIPT_CONTROL: &[&str] =
    &["true", "false", "null", "undefined", "this", "globalThis", "NaN", "Infinity"];
pub(super) static TYPESCRIPT_BUILTINS: &[&str] = &[
    "console", "log", "warn", "error", "info", "debug", "Array", "Map", "Set", "WeakMap",
    "WeakSet", "Promise", "Object", "String", "Number", "Boolean", "Symbol", "Date", "RegExp",
    "JSON", "Math", "Error", "TypeError", "RangeError", "Buffer", "process", "globalThis",
    "Int8Array", "Uint8Array", "Int16Array", "Uint16Array", "Int32Array", "Uint32Array",
    "Float32Array", "Float64Array", "BigInt", "BigInt64Array", "BigUint64Array", "matlab_runtime",
    "NDArray", "linspace", "rand", "matforge",
];

pub(super) static VERILOG_KEYWORDS: &[&str] = &[
    "module", "endmodule", "input", "output", "inout", "wire", "reg", "logic", "bit", "byte",
    "shortint", "int", "longint", "integer", "real", "shortreal", "time", "string", "always",
    "always_ff", "always_comb", "always_latch", "posedge", "negedge", "edge", "begin", "end",
    "if", "else", "for", "while", "do", "repeat", "forever", "parameter", "localparam",
    "function", "endfunction", "task", "endtask", "case", "casez", "casex", "endcase", "default",
    "assign", "deassign", "initial", "final", "generate", "endgenerate", "genvar", "typedef",
    "struct", "union", "enum", "packed", "unpacked", "interface", "endinterface", "modport",
    "class", "endclass", "extends", "virtual", "new", "this", "super", "return", "break",
    "continue", "import", "export", "package", "endpackage", "property", "endproperty",
    "sequence", "endsequence", "assert", "assume", "cover", "wait", "fork", "join", "join_any",
    "join_none", "disable", "force", "release", "signed", "unsigned", "static", "automatic",
    "const", "ref", "var", "rand", "randc", "constraint", "covergroup", "endcovergroup",
    "coverpoint", "cross", "analog", "electrical", "ground", "branch", "nature", "discipline",
    "enddiscipline", "from", "exclude",
];
pub(super) static VERILOG_BUILTINS: &[&str] = &[
    "$display", "$write", "$monitor", "$strobe", "$time", "$realtime", "$finish", "$stop",
    "$random", "$urandom", "$urandom_range", "$signed", "$unsigned", "$bits", "$clog2", "$onehot",
    "$onehot0", "$isunknown", "$cast", "$readmemh", "$readmemb", "$writememh", "$writememb",
    "$dumpfile", "$dumpvars", "$past", "$rose", "$fell", "$stable", "$changed", "$abstime",
    "$temperature", "$table_model", "ddt", "idt", "idtmod", "absdelay", "laplace_nd",
    "laplace_np", "laplace_zd", "laplace_zp", "transition", "cross", "above", "timer",
    "last_crossing", "slew", "limexp", "white_noise", "flicker_noise", "noise_table",
];

pub(super) static LLVM_KEYWORDS: &[&str] = &[
    "define", "declare", "ret", "br", "call", "invoke", "load", "store", "getelementptr",
    "alloca", "phi", "select", "switch", "unreachable", "resume", "cleanup", "catchpad",
    "catchret", "catchswitch", "freeze", "fence", "atomicrmw", "cmpxchg", "extractvalue",
    "insertvalue", "extractelement", "insertelement", "shufflevector", "fadd", "fsub", "fmul",
    "fdiv", "frem", "fneg", "icmp", "fcmp", "add", "sub", "mul", "sdiv", "udiv", "srem", "urem",
    "shl", "lshr", "ashr", "and", "or", "xor", "trunc", "zext", "sext", "fpext", "fptrunc",
    "fptoui", "fptosi", "uitofp", "sitofp", "bitcast", "ptrtoint", "inttoptr", "addrspacecast",
    "i1", "i8", "i16", "i32", "i64", "i128", "i256", "f16", "bfloat", "f32", "f64", "f128",
    "x86_fp80", "ppc_fp128", "double", "float", "void", "ptr", "label", "metadata", "token",
    "x86_mmx", "opaque", "type", "nuw", "nsw", "fast", "inbounds", "nocapture", "noalias",
    "readonly", "readnone", "writeonly", "dereferenceable", "dereferenceable_or_null", "nonnull",
    "aligned", "byval", "sret", "inreg", "signext", "zeroext", "noreturn", "nounwind", "cold",
    "hot", "minsize", "optsize", "norecurse", "willreturn", "alwaysinline", "noinline", "musttail",
    "tail", "notail", "dso_local", "dso_preemptable", "weak", "internal", "linkonce",
    "linkonce_odr", "weak_odr", "external", "extern_weak", "appending", "private",
    "available_externally", "common", "target", "datalayout", "triple", "source_filename",
    "module", "attributes", "comdat", "uselistorder", "uselistorder_bb",
];
pub(super) static LLVM_CONTROL: &[&str] =
    &["true", "false", "null", "undef", "poison", "zeroinitializer", "none"];

pub(super) static MLIR_KEYWORDS: &[&str] = &[
    "module", "func", "return", "memref", "tensor", "vector", "i1", "i8", "i16", "i32", "i64",
    "i128", "f16", "f32", "f64", "f128", "bf16", "index", "scf", "arith", "linalg", "affine",
    "loc", "constant", "yield", "loop", "for", "while", "if", "else", "do",
];
pub(super) static MLIR_CONTROL: &[&str] = &["true", "false"];
