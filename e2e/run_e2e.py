#!/usr/bin/env python3
"""MatForge IDE end-to-end tests.

Each scenario launches the real binary, drives it with synthesized input, and
asserts on the app's published state. Run via `just e2e` (builds first).
"""

import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import App, check, summary_and_exit  # noqa: E402

PROJ = "/tmp/mf_e2e_proj"
MAIN = os.path.join(PROJ, "main.m")
MATLABC = os.environ.get("MATLABC_PATH", "/home/leonardo/work/matlab_llvm/build/matlabc")
FIXTURES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures")
SIGNAL_MFLOW = os.path.join(FIXTURES, "signal.mflow")
CHART_MFLOW = os.path.join(FIXTURES, "chart.mflow")


def setup_project():
    os.makedirs(PROJ, exist_ok=True)
    with open(MAIN, "w") as f:
        f.write("a = 3;\nb = a + 4;\nc = b * 2;\ndisp(c)\n")


def scenario_gutter_breakpoint():
    print("scenario: toggle a breakpoint by clicking the gutter")
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATFORGE_FILE": MAIN})
    try:
        app.wait_for(lambda s: s.get("active_tab") == "main.m", what="main.m open")
        gx, gy, gw, gh = app.wait_rect("gutter_rect")

        app.click_window(gx + gw // 2, gy + 50)          # click a line in the gutter
        st = app.wait_for(lambda s: s["active_breakpoints"], what="breakpoint set")
        check("gutter click sets a breakpoint", bool(st["active_breakpoints"]),
              f"lines={st['active_breakpoints']}")

        line = st["active_breakpoints"][0]
        app.click_window(gx + gw // 2, gy + 50)          # click same spot → toggle off
        st = app.wait_for(lambda s: line not in s["active_breakpoints"], what="breakpoint cleared")
        check("clicking again clears it", line not in st["active_breakpoints"])
    finally:
        app.close()


def scenario_f9_breakpoint():
    print("scenario: toggle a breakpoint with F9 at the cursor")
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATFORGE_FILE": MAIN})
    try:
        app.wait_for(lambda s: s.get("active_tab") == "main.m", what="main.m open")
        gx, gy, gw, gh = app.wait_rect("gutter_rect")
        app.click_window(gx + gw + 60, gy + 50)          # focus editor + place cursor
        app.key("F9")
        st = app.wait_for(lambda s: s["active_breakpoints"], what="F9 breakpoint")
        check("F9 sets a breakpoint", bool(st["active_breakpoints"]),
              f"lines={st['active_breakpoints']}")
    finally:
        app.close()


def scenario_repl_workspace():
    print("scenario: live REPL command updates the workspace")
    if not os.path.exists(MATLABC):
        check("REPL → workspace (skipped: matlabc not found)", True, "skipped")
        return
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATLABC_PATH": MATLABC})
    try:
        ex, ey, ew, eh = app.wait_rect("repl_entry_rect")
        app.click_window(ex + ew // 2, ey + eh // 2)     # focus the REPL entry
        app.type_text("x = [1 2 3]")
        app.key("Return")
        st = app.wait_for(lambda s: "x" in s.get("workspace", []), timeout=20,
                          what="workspace variable x")
        check("REPL command creates workspace var 'x'", "x" in st["workspace"],
              f"workspace={st['workspace']}")
    finally:
        app.close()


def scenario_inspect_and_plot():
    print("scenario: inspect a workspace variable and plot it")
    if not os.path.exists(MATLABC):
        check("inspect + plot (skipped: matlabc not found)", True, "skipped")
        return
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATLABC_PATH": MATLABC})
    try:
        ex, ey, ew, eh = app.wait_rect("repl_entry_rect")
        app.click_window(ex + ew // 2, ey + eh // 2)
        app.type_text("M = [1 2 3 4]")
        app.key("Return")
        app.wait_for(lambda s: "M" in s.get("workspace", []), timeout=20, what="var M")

        # Click the first workspace row (M) -> capture its value.
        tx, ty, tw, th = app.wait_rect("workspace_table_rect")
        app.click_window(tx + 30, ty + 12)
        st = app.wait_for(lambda s: s.get("inspected_matrix"), timeout=15, what="value captured")
        m = st["inspected_matrix"]
        check("clicking a variable shows its value", m is not None and m["cols"] == 4,
              f"inspected={m}")

        # Click Plots '+' -> plot the inspected variable.
        before = app.state().get("plots", 0)
        px, py, pw, ph = app.wait_rect("plots_add_rect")
        app.click_window(px + pw // 2, py + ph // 2)
        st = app.wait_for(lambda s: s.get("plots", 0) > before, what="figure added")
        check("plotting the variable adds a figure", st["plots"] > before,
              f"plots={st['plots']}")
    finally:
        app.close()


def scenario_repl_plot():
    print("scenario: plot() in the REPL produces a figure")
    if not os.path.exists(MATLABC):
        check("REPL plot (skipped: matlabc not found)", True, "skipped")
        return
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATLABC_PATH": MATLABC})
    try:
        ex, ey, ew, eh = app.wait_rect("repl_entry_rect")
        app.click_window(ex + ew // 2, ey + eh // 2)
        app.type_text("plot([1 2 3 4 3 2 1])")
        app.key("Return")
        st = app.wait_for(lambda s: s.get("plots", 0) > 0, timeout=25, what="figure from plot()")
        check("plot() in the REPL adds a figure", st["plots"] > 0, f"plots={st['plots']}")
    finally:
        app.close()


def scenario_search():
    print("scenario: find-in-files lists matches and opens one")
    # Open the Search panel deterministically via the env hook instead of the
    # Ctrl+F accelerator, which is flaky to deliver under headless Xvfb.
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATFORGE_SEARCH": "1"})
    try:
        sx, sy, sw, sh = app.wait_rect("search_entry_rect")
        app.click_window(sx + sw // 2, sy + sh // 2)
        app.type_text("disp")
        app.key("Return")
        st = app.wait_for(lambda s: s.get("search_results", 0) > 0, timeout=10,
                          what="search results")
        check("searching 'disp' returns results", st["search_results"] > 0,
              f"results={st['search_results']}")
    finally:
        app.close()


def scenario_problems_pane():
    print("scenario: compiling a bad file populates PROBLEMS")
    if not os.path.exists(MATLABC):
        check("PROBLEMS (skipped: matlabc not found)", True, "skipped")
        return
    bad = os.path.join(PROJ, "bad.m")
    with open(bad, "w") as f:
        f.write("x = 1 + + undefined_name_zzz;\n")
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATFORGE_FILE": bad,
                         "MATFORGE_COMPILE": "1", "MATLABC_PATH": MATLABC})
    try:
        st = app.wait_for(lambda s: s.get("problems", 0) > 0, timeout=20, what="diagnostics")
        check("a bad compile adds PROBLEMS diagnostics", st["problems"] > 0,
              f"problems={st['problems']}")
    finally:
        app.close()


def scenario_explorer_double_click():
    print("scenario: Explorer opens a file on double-click, not single-click")
    proj = "/tmp/mf_e2e_explore"
    os.makedirs(proj, exist_ok=True)
    with open(os.path.join(proj, "zzz.m"), "w") as f:
        f.write("q = 1;\n")
    app = App(env_extra={"MATFORGE_OPEN": proj})
    try:
        lx, ly, lw, lh = app.wait_rect("explorer_list_rect")
        rx, ry = lx + 30, ly + 12                        # first row near the top
        app.click_window(rx, ry)                         # single click: select only
        time.sleep(0.8)
        st = app.state()
        check("single click does NOT open the file", "zzz.m" not in st.get("tabs", []),
              f"tabs={st.get('tabs')}")
        app.double_click_window(rx, ry)                  # double click: open
        st = app.wait_for(lambda s: "zzz.m" in s.get("tabs", []), timeout=8,
                          what="file opened on double-click")
        check("double click opens the file", "zzz.m" in st.get("tabs", []), f"tabs={st.get('tabs')}")
    finally:
        app.close()


def scenario_debug_session():
    print("scenario: debug session pauses at a breakpoint, steps, and evaluates a watch")
    if not os.path.exists(MATLABC):
        check("debug session (skipped: matlabc not found)", True, "skipped")
        return
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATFORGE_FILE": MAIN,
                         "MATFORGE_BP": "2", "MATFORGE_DEBUG": "1", "MATLABC_PATH": MATLABC})
    try:
        st = app.wait_for(lambda s: s.get("debug_state") == "Paused", timeout=25,
                          what="paused at breakpoint")
        check("debug session pauses at the breakpoint", st.get("debug_state") == "Paused",
              f"line={st.get('execution_line')}")
        line0 = st.get("execution_line")

        # Step over (toolbar button — app accelerators don't fire under XTEST).
        nx, ny, nw, nh = app.wait_rect("debug_next_rect")
        app.click_window(nx + nw // 2, ny + nh // 2)
        st = app.wait_for(lambda s: s.get("execution_line") and s.get("execution_line") != line0,
                          timeout=15, what="step over advances the line")
        check("step over advances the execution line", st.get("execution_line") != line0,
              f"{line0} -> {st.get('execution_line')}")

        # Evaluate a watch expression on the paused frame.
        wx, wy, ww, wh = app.wait_rect("watch_entry_rect")
        app.click_window(wx + ww // 2, wy + wh // 2)
        app.type_text("a")
        app.key("Return")
        st = app.wait_for(lambda s: s.get("watch", 0) > 0, timeout=15, what="watch result")
        check("watch evaluates an expression", st.get("watch", 0) > 0, f"watch={st.get('watch')}")

        # Continue — the session should run to completion.
        cx, cy, cw, ch = app.wait_rect("debug_continue_rect")
        app.click_window(cx + cw // 2, cy + ch // 2)
        st = app.wait_for(lambda s: s.get("debug_state") in ("Terminated", "Idle"),
                          timeout=20, what="debug session ends")
        check("continue ends the debug session", st.get("debug_state") in ("Terminated", "Idle"),
              f"state={st.get('debug_state')}")
    finally:
        app.close()


def scenario_plot_animation():
    print("scenario: a plotted vector is a scrub-able animation (trace reveal)")
    if not os.path.exists(MATLABC):
        check("plot animation (skipped: matlabc not found)", True, "skipped")
        return
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATLABC_PATH": MATLABC})
    try:
        ex, ey, ew, eh = app.wait_rect("repl_entry_rect")
        app.click_window(ex + ew // 2, ey + eh // 2)
        app.type_text("V = [1 2 3 4 5 6]")
        app.key("Return")
        app.wait_for(lambda s: "V" in s.get("workspace", []), timeout=20, what="var V")

        tx, ty, tw, th = app.wait_rect("workspace_table_rect")
        app.click_window(tx + 30, ty + 12)               # inspect V
        app.wait_for(lambda s: s.get("inspected_matrix"), timeout=15, what="inspected V")

        px, py, pw, ph = app.wait_rect("plots_add_rect")
        app.click_window(px + pw // 2, py + ph // 2)      # plot V
        st = app.wait_for(lambda s: s.get("plot_anim", 0) > 1, timeout=15,
                          what="scrub-able figure (animation_len>1)")
        check("a plotted vector is scrub-able (animation_len>1)", st.get("plot_anim", 0) > 1,
              f"plot_anim={st.get('plot_anim')}")

        # The playback bar appears; clicking play must not crash and keeps the figure.
        bx, by, bw, bh = app.wait_rect("plots_play_rect", timeout=8)
        app.click_window(bx + bw // 2, by + bh // 2)
        st = app.wait_for(lambda s: s.get("plots", 0) >= 1, timeout=5, what="figure survives playback")
        check("the animation play button is live", st.get("plots", 0) >= 1, f"plots={st.get('plots')}")
    finally:
        app.close()


def scenario_flowchart_editor():
    print("scenario: the flowchart editor opens a chart and the palette adds nodes")
    # MATFORGE_NEWFLOW opens a demo control-flow chart in a tab (no matlabc needed).
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATFORGE_NEWFLOW": "control"})
    try:
        st = app.wait_for(lambda s: (s.get("flowchart") or {}).get("nodes", 0) > 0,
                          timeout=10, what="flowchart loaded with nodes")
        fc = st["flowchart"]
        check("the demo flowchart loads with nodes and edges",
              fc["nodes"] > 0 and fc["edges"] > 0, f"flowchart={fc}")

        # Click the first BLOCKS palette row -> the view model gains a node.
        before = fc["nodes"]
        px, py, pw, ph = app.wait_rect("flowchart_palette_rect")
        app.click_window(px + pw // 2, py + 12)
        st = app.wait_for(lambda s: (s.get("flowchart") or {}).get("nodes", 0) > before,
                          timeout=8, what="palette click adds a node")
        check("clicking a palette block adds a node",
              st["flowchart"]["nodes"] > before,
              f"{before} -> {st['flowchart']['nodes']}")
    finally:
        app.close()


def scenario_signal_editor_features():
    """The signal-flow editor's new interactions, driven deterministically via
    MATFORGE_FLOW_OP (no matlabc needed, so this runs in CI). The demo signal
    chart is Constant -> Gain -> MATLAB Function -> Scope (3 wires)."""
    print("scenario: signal-flow editor wire select/delete, breakpoints, MATLAB Function ports")

    def open_signal(op):
        return App(env_extra={"MATFORGE_OPEN": PROJ, "MATFORGE_NEWFLOW": "signal",
                              "MATFORGE_FLOW_OP": op})

    # Baseline: the demo loads with a MATLAB Function block (1 input) and 3 wires.
    app = open_signal("")
    try:
        st = app.wait_for(lambda s: (s.get("flowchart") or {}).get("nodes", 0) >= 4,
                          timeout=10, what="signal demo loaded")
        fc = st["flowchart"]
        check("the signal demo loads its blocks and wires",
              fc["nodes"] >= 4 and fc["edges"] == 3, f"flowchart={fc}")
        check("the MATLAB Function block starts with one input port",
              fc.get("matlab_inputs") == 1, f"matlab_inputs={fc.get('matlab_inputs')}")
    finally:
        app.close()

    # Selecting a wire publishes its id.
    app = open_signal("select-edge")
    try:
        st = app.wait_for(lambda s: (s.get("flowchart") or {}).get("selected_edge"),
                          timeout=10, what="a wire is selected")
        check("clicking a wire selects it",
              bool(st["flowchart"].get("selected_edge")),
              f"selected_edge={st['flowchart'].get('selected_edge')}")
    finally:
        app.close()

    # Deleting the selected wire drops the edge count.
    app = open_signal("delete-edge")
    try:
        st = app.wait_for(lambda s: (s.get("flowchart") or {}).get("edges", 9) < 3,
                          timeout=10, what="a wire is deleted")
        check("deleting a selected wire removes it",
              st["flowchart"]["edges"] == 2, f"edges={st['flowchart']['edges']}")
    finally:
        app.close()

    # A per-wire breakpoint is recorded on the edge.
    app = open_signal("set-edge-bp")
    try:
        st = app.wait_for(lambda s: (s.get("flowchart") or {}).get("edge_breakpoints", 0) > 0,
                          timeout=10, what="a wire breakpoint is set")
        check("a per-wire breakpoint is stored on the edge",
              st["flowchart"]["edge_breakpoints"] == 1,
              f"edge_breakpoints={st['flowchart']['edge_breakpoints']}")
    finally:
        app.close()

    # Opening the MATLAB Function source editor publishes its line-number gutter.
    app = open_signal("open-matlab")
    try:
        st = app.wait_for(lambda s: s.get("gutter_rect"),
                          timeout=10, what="MATLAB Function editor opens with a gutter")
        check("double-click opens the MATLAB Function editor (with a line gutter)",
              bool(st.get("gutter_rect")), f"gutter_rect={st.get('gutter_rect')}")
    finally:
        app.close()

    # Growing the MATLAB Function signature grows its input ports to u1..u3.
    app = open_signal("grow-matlab")
    try:
        st = app.wait_for(lambda s: (s.get("flowchart") or {}).get("matlab_inputs", 0) >= 3,
                          timeout=10, what="MATLAB Function ports follow the signature")
        check("MATLAB Function input ports follow the function signature",
              st["flowchart"]["matlab_inputs"] == 3,
              f"matlab_inputs={st['flowchart']['matlab_inputs']}")
    finally:
        app.close()


def scenario_mflowlink_simulate():
    print("scenario: the mflowLink window runs a signal-flow simulation")
    if not os.path.exists(MATLABC):
        check("mflowLink simulate (skipped: matlabc not found)", True, "skipped")
        return
    # MATFORGE_SIMULATE opens the standalone window and autostarts the run; we
    # assert on the published simulation state (no input needed).
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATFORGE_SIMULATE": SIGNAL_MFLOW,
                         "MATLABC_PATH": MATLABC})
    try:
        st = app.wait_for(lambda s: (s.get("mflowlink") or {}).get("samples", 0) > 0,
                          timeout=30, what="simulation produced samples")
        ml = st["mflowlink"]
        check("the signal-flow simulation streams samples", ml["samples"] > 0,
              f"mflowlink={ml}")
        st = app.wait_for(lambda s: (s.get("mflowlink") or {}).get("state") == "Finished",
                          timeout=30, what="simulation finishes")
        check("the simulation reaches the Finished state",
              st["mflowlink"]["state"] == "Finished", f"state={st['mflowlink']['state']}")
    finally:
        app.close()


def scenario_statechart_trace():
    print("scenario: the mStateflow window traces a state chart")
    if not os.path.exists(MATLABC):
        check("mStateflow trace (skipped: matlabc not found)", True, "skipped")
        return
    app = App(env_extra={"MATFORGE_OPEN": PROJ, "MATFORGE_STATECHART": CHART_MFLOW,
                         "MATLABC_PATH": MATLABC})
    try:
        st = app.wait_for(lambda s: (s.get("statechart") or {}).get("events", 0) > 0,
                          timeout=30, what="trace produced events")
        sc = st["statechart"]
        check("the state-chart trace streams events", sc["events"] > 0,
              f"statechart={sc}")
        st = app.wait_for(lambda s: (s.get("statechart") or {}).get("active", 0) > 0,
                          timeout=30, what="a state becomes active")
        check("the trace activates at least one state",
              st["statechart"]["active"] > 0, f"active={st['statechart']['active']}")
    finally:
        app.close()


def main():
    setup_project()
    scenario_search()
    scenario_problems_pane()
    scenario_gutter_breakpoint()
    scenario_f9_breakpoint()
    scenario_explorer_double_click()
    scenario_flowchart_editor()
    scenario_signal_editor_features()
    scenario_mflowlink_simulate()
    scenario_statechart_trace()
    scenario_repl_workspace()
    scenario_inspect_and_plot()
    scenario_repl_plot()
    scenario_plot_animation()
    scenario_debug_session()
    summary_and_exit()


if __name__ == "__main__":
    main()
