#!/usr/bin/env python3
"""MatForge IDE end-to-end tests.

Each scenario launches the real binary, drives it with synthesized input, and
asserts on the app's published state. Run via `just e2e` (builds first).
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import App, check, summary_and_exit  # noqa: E402

PROJ = "/tmp/mf_e2e_proj"
MAIN = os.path.join(PROJ, "main.m")
MATLABC = os.environ.get("MATLABC_PATH", "/home/leonardo/work/matlab_llvm/build/matlabc")


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


def main():
    setup_project()
    scenario_problems_pane()
    scenario_gutter_breakpoint()
    scenario_f9_breakpoint()
    scenario_repl_workspace()
    scenario_inspect_and_plot()
    scenario_repl_plot()
    summary_and_exit()


if __name__ == "__main__":
    main()
