//! # matforge-core
//!
//! The GTK-free heart of the MatForge IDE — a Linux/Rust+GTK4 port of the macOS
//! SwiftUI IDE for the `matlab_llvm` compiler. This crate holds the three lower
//! MVVM layers so they can be unit-tested without a display:
//!
//! * [`models`] — pure value types (project tree, editor tabs, flowchart docs,
//!   plots, compiler config, DAP types). No I/O, no framework imports.
//! * [`services`] — trait-abstracted side effects (compiler/REPL/DAP processes,
//!   file system, syntax highlighting, `.mflow` codec, output parsers) plus
//!   in-crate fakes for tests.
//! * [`viewmodels`] — reactive state + verb methods built on [`observable`],
//!   depending only on service *traits* so tests inject fakes.
//!
//! The `matforge` binary crate (`crates/app`) supplies the GTK views and wires
//! them to these view models. See `docs/architecture.md`.

pub mod models;
pub mod observable;
pub mod services;
pub mod theme;
pub mod viewmodels;

pub use observable::{Property, SubscriptionId};
