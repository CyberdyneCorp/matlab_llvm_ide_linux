//! Side-effecting services: each is a pure-logic core (parsers, codecs, argv
//! builders, protocol framing) plus, where a process or the file system is
//! involved, a trait with a real implementation and an in-crate fake. View
//! models depend only on the traits, so they unit-test against the fakes.

pub mod compiler;
pub mod dap;
pub mod filesystem;
pub mod flowchart_codec;
pub mod highlighter;
pub mod parsers;
pub mod run;
pub mod sentinels;
pub mod settings;
pub mod system_bridge;
