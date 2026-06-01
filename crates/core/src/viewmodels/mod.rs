//! Reactive view models — `@MainActor ObservableObject` equivalents built on
//! [`Property`](crate::observable::Property). Each holds observable state and
//! verb methods, depends only on service *traits*, and is unit-tested against
//! the in-crate fakes. The GTK views (app crate) subscribe to the properties
//! and call the verb methods. `MainViewModel` is the composition root.

pub mod activity_bar;
pub mod breakpoints;
pub mod console;
pub mod debug;
pub mod editor;
pub mod flowchart;
pub mod layout;
pub mod main;
pub mod plots;
pub mod project_explorer;
pub mod repl;
pub mod search;
pub mod status_bar;
pub mod toolbar;
pub mod workspace;

pub use activity_bar::{ActivityBarViewModel, ActivityItem};
pub use breakpoints::BreakpointsViewModel;
pub use console::ConsoleViewModel;
pub use debug::{DebugState, DebugViewModel};
pub use editor::EditorViewModel;
pub use flowchart::FlowchartViewModel;
pub use layout::LayoutViewModel;
pub use main::MainViewModel;
pub use plots::PlotsViewModel;
pub use project_explorer::ProjectExplorerViewModel;
pub use repl::ReplViewModel;
pub use search::{SearchResult, SearchViewModel};
pub use status_bar::StatusBarViewModel;
pub use toolbar::ToolbarViewModel;
pub use workspace::WorkspaceViewModel;
