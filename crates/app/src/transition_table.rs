//! State-transition table editor: a tabular alternative to drawing transitions
//! on the chart canvas. Each row is one `Transition` edge decomposed into
//! source × dest × event × guard × cond-action × trans-action × priority, edited
//! through the tested `FlowchartViewModel` (edge ids preserved, so the table and
//! canvas stay in sync). This is GTK glue.

use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, DropDown, Entry, Label, Orientation, ScrolledWindow, Window};

use matforge_core::models::flowchart::{NodeKind, TransitionLabel};
use matforge_core::viewmodels::FlowchartViewModel;

use crate::app_state::AppState;

const COLUMNS: [&str; 8] = [
    "Source",
    "Dest",
    "Event",
    "Guard",
    "Cond action",
    "Trans action",
    "Priority",
    "",
];

/// Open the state-transition table window for a chart document.
pub fn open(app: &Rc<AppState>, fc: &Rc<FlowchartViewModel>) {
    let window = Window::builder()
        .title("State Transition Table")
        .default_width(920)
        .default_height(480)
        .build();
    window.add_css_class("mf-root");

    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("mf-window");

    let grid = gtk::Grid::new();
    grid.set_row_spacing(4);
    grid.set_column_spacing(8);
    grid.set_margin_top(8);
    grid.set_margin_bottom(8);
    grid.set_margin_start(10);
    grid.set_margin_end(10);

    let scroll = ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&grid));

    // Rebuild the whole grid from the current transitions (called on open and
    // after structural edits: add / delete a row).
    let rebuild: Rc<dyn Fn()> = {
        let grid = grid.clone();
        let fc = fc.clone();
        let app = app.clone();
        Rc::new(move || rebuild_grid(&grid, &fc, &app))
    };

    let toolbar = GtkBox::new(Orientation::Horizontal, 6);
    toolbar.set_margin_top(6);
    toolbar.set_margin_start(10);
    let add = Button::with_label("+ Transition");
    add.add_css_class("mf-tool");
    add.add_css_class("mf-run");
    {
        let fc = fc.clone();
        let app = app.clone();
        let rebuild = rebuild.clone();
        add.connect_clicked(move |_| {
            let states = state_list(&fc);
            let Some((first, _)) = states.first() else {
                app.vm.toast.show("Add at least two states first");
                return;
            };
            let dst = states
                .get(1)
                .map(|s| s.0.clone())
                .unwrap_or_else(|| first.clone());
            fc.add_transition(first, &dst);
            rebuild();
        });
    }
    toolbar.append(&add);
    root.append(&toolbar);
    root.append(&scroll);
    window.set_child(Some(&root));

    rebuild();
    window.present();
}

/// The chart's `State` nodes as `(id, display label)` pairs for the dropdowns.
fn state_list(fc: &Rc<FlowchartViewModel>) -> Vec<(String, String)> {
    let idx = fc.current_flow_index();
    fc.document.with(|d| {
        d.flows
            .get(idx)
            .map(|f| {
                f.nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::State)
                    .map(|n| {
                        let label = if n.label.is_empty() {
                            n.id.clone()
                        } else {
                            n.label.clone()
                        };
                        (n.id.clone(), label)
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn rebuild_grid(grid: &gtk::Grid, fc: &Rc<FlowchartViewModel>, app: &Rc<AppState>) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
    for (col, title) in COLUMNS.iter().enumerate() {
        let lbl = Label::new(Some(title));
        lbl.add_css_class("mf-col-title");
        lbl.set_halign(gtk::Align::Start);
        grid.attach(&lbl, col as i32, 0, 1, 1);
    }

    let states = state_list(fc);
    let rows = fc.transition_rows();
    if rows.is_empty() {
        let empty = Label::new(Some("No transitions yet — add one below."));
        empty.add_css_class("mf-text-muted");
        empty.set_halign(gtk::Align::Start);
        grid.attach(&empty, 0, 1, COLUMNS.len() as i32, 1);
        return;
    }
    for (i, row) in rows.into_iter().enumerate() {
        build_row(grid, fc, app, &states, &row, i as i32 + 1);
    }
}

/// Build one editable row. The `commit` closure reads every widget back and
/// writes the edge through the view model (id preserved).
fn build_row(
    grid: &gtk::Grid,
    fc: &Rc<FlowchartViewModel>,
    app: &Rc<AppState>,
    states: &[(String, String)],
    row: &matforge_core::viewmodels::flowchart::TransitionRow,
    r: i32,
) {
    let ids: Vec<String> = states.iter().map(|s| s.0.clone()).collect();
    let names: Vec<&str> = states.iter().map(|s| s.1.as_str()).collect();
    let index_of = |id: &str| ids.iter().position(|x| x == id).unwrap_or(0) as u32;

    let src_dd = DropDown::from_strings(&names);
    src_dd.set_selected(index_of(&row.src));
    let dst_dd = DropDown::from_strings(&names);
    dst_dd.set_selected(index_of(&row.dst));

    let entry = |text: Option<&str>, chars: i32| {
        let e = Entry::new();
        e.set_text(text.unwrap_or(""));
        e.set_width_chars(chars);
        e
    };
    let event = entry(row.label.event.as_deref(), 8);
    let guard = entry(row.label.guard.as_deref(), 12);
    let cond = entry(row.label.cond_action.as_deref(), 12);
    let trans = entry(row.label.trans_action.as_deref(), 12);
    let prio = entry(row.priority.map(|p| p.to_string()).as_deref(), 4);

    // Commit reads every field and writes the edge (id preserved).
    let commit: Rc<dyn Fn()> = {
        let fc = fc.clone();
        let edge_id = row.edge_id.clone();
        let ids = ids.clone();
        let (src_dd, dst_dd) = (src_dd.clone(), dst_dd.clone());
        let (event, guard, cond, trans, prio) = (
            event.clone(),
            guard.clone(),
            cond.clone(),
            trans.clone(),
            prio.clone(),
        );
        Rc::new(move || {
            let pick = |dd: &DropDown| ids.get(dd.selected() as usize).cloned().unwrap_or_default();
            let opt = |e: &Entry| {
                let t = e.text().trim().to_string();
                (!t.is_empty()).then_some(t)
            };
            let label = TransitionLabel {
                event: opt(&event),
                guard: opt(&guard),
                cond_action: opt(&cond),
                trans_action: opt(&trans),
            };
            let priority = prio.text().trim().parse::<u32>().ok();
            fc.set_transition(&edge_id, &pick(&src_dd), &pick(&dst_dd), &label, priority);
        })
    };

    // Dropdowns commit immediately; text fields commit when focus leaves (one
    // undo step per field edit, no per-keystroke churn).
    for dd in [&src_dd, &dst_dd] {
        let commit = commit.clone();
        dd.connect_selected_notify(move |_| commit());
    }
    for e in [&event, &guard, &cond, &trans, &prio] {
        let focus = gtk::EventControllerFocus::new();
        let on_leave = commit.clone();
        focus.connect_leave(move |_| on_leave());
        e.add_controller(focus);
        let on_activate = commit.clone();
        e.connect_activate(move |_| on_activate());
    }

    let del = Button::from_icon_name("user-trash-symbolic");
    del.add_css_class("flat");
    del.set_tooltip_text(Some("Delete this transition"));
    {
        let fc = fc.clone();
        let app = app.clone();
        let edge_id = row.edge_id.clone();
        let grid = grid.clone();
        del.connect_clicked(move |_| {
            fc.delete_edge(&edge_id);
            rebuild_grid(&grid, &fc, &app);
        });
    }

    let widgets: [&gtk::Widget; 8] = [
        src_dd.upcast_ref(),
        dst_dd.upcast_ref(),
        event.upcast_ref(),
        guard.upcast_ref(),
        cond.upcast_ref(),
        trans.upcast_ref(),
        prio.upcast_ref(),
        del.upcast_ref(),
    ];
    for (col, w) in widgets.iter().enumerate() {
        grid.attach(*w, col as i32, r, 1, 1);
    }
}
