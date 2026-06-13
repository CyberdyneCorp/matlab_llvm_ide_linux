"""End-to-end harness for MatForge IDE.

Drives the *real* `matforge` binary with synthesized X11 input (XTEST) and
asserts on real application state, which the app publishes as a JSON snapshot
(see crates/app/src/e2e.rs) when `MATFORGE_E2E_STATE` is set. The snapshot also
carries the on-screen rectangles of drive targets (editor gutter, REPL entry),
so clicks hit real coordinates rather than hardcoded guesses.

Requires a running X display and `python-xlib` (`pip install --user python-xlib`).
Headless CI: wrap with `xvfb-run`.
"""

import json
import os
import subprocess
import time

from Xlib import X, XK, display
from Xlib.ext import xtest

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BINARY = os.path.join(REPO, "target", "debug", "matforge")


class App:
    """A launched MatForge instance with input + state-introspection helpers."""

    def __init__(self, env_extra=None, state_path="/tmp/matforge_e2e_state.json"):
        self.state_path = state_path
        self.disp = display.Display()
        # Ensure no stale instance steals the window lookup.
        subprocess.run(["pkill", "-9", "-x", "matforge"], stderr=subprocess.DEVNULL)
        time.sleep(0.8)
        if os.path.exists(state_path):
            os.remove(state_path)
        env = dict(os.environ)
        env["MATFORGE_E2E_STATE"] = state_path
        # Hermetic config: a fresh XDG_CONFIG_HOME so a developer's saved layout
        # (e.g. a hidden Plots/Workspace panel) can't change which panels are
        # visible and break panel-rect lookups. Defaults → all panels shown.
        cfg = state_path + ".config"
        subprocess.run(["rm", "-rf", cfg], stderr=subprocess.DEVNULL)
        env["XDG_CONFIG_HOME"] = cfg
        if env_extra:
            env.update(env_extra)
        # Detach so it survives this process; we kill the group on close.
        self.proc = subprocess.Popen(
            [BINARY], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        self.win = self._await_window(timeout=15)
        self.origin = self._window_origin()

    # ---- lifecycle --------------------------------------------------------

    def _await_window(self, timeout):
        deadline = time.time() + timeout
        while time.time() < deadline:
            w = self._find(self.disp.screen().root)
            if w is not None:
                time.sleep(0.6)  # let it lay out
                return w
            time.sleep(0.2)
        raise TimeoutError("MatForge window did not appear")

    def _find(self, w):
        try:
            name = w.get_wm_name()
        except Exception:
            name = None
        if name and "MatForge" in str(name):
            return w
        try:
            for c in w.query_tree().children:
                r = self._find(c)
                if r:
                    return r
        except Exception:
            pass
        return None

    def _window_origin(self):
        t = self.win.translate_coords(self.disp.screen().root, 0, 0)
        return (-t.x, -t.y)  # absolute screen coords of the client top-left

    def close(self):
        try:
            self.proc.terminate()
            self.proc.wait(timeout=3)
        except Exception:
            try:
                os.killpg(os.getpgid(self.proc.pid), 9)
            except Exception:
                pass

    # ---- state ------------------------------------------------------------

    def state(self):
        try:
            with open(self.state_path) as f:
                return json.load(f)
        except Exception:
            return {}

    def wait_for(self, predicate, timeout=10, what="condition"):
        deadline = time.time() + timeout
        last = None
        while time.time() < deadline:
            last = self.state()
            try:
                if predicate(last):
                    return last
            except Exception:
                pass
            time.sleep(0.15)
        raise AssertionError(f"timed out waiting for {what}; last state={last}")

    def wait_rect(self, key, timeout=10):
        st = self.wait_for(lambda s: s.get(key), timeout, what=key)
        return st[key]

    # ---- input ------------------------------------------------------------

    def _screen_xy(self, win_x, win_y):
        return self.origin[0] + win_x, self.origin[1] + win_y

    def click_window(self, win_x, win_y):
        self.origin = self._window_origin()  # re-read in case the WM moved it
        x, y = self._screen_xy(win_x, win_y)
        xtest.fake_input(self.disp, X.MotionNotify, x=x, y=y); self.disp.sync(); time.sleep(0.05)
        xtest.fake_input(self.disp, X.ButtonPress, 1); self.disp.sync(); time.sleep(0.05)
        xtest.fake_input(self.disp, X.ButtonRelease, 1); self.disp.sync(); time.sleep(0.15)

    def double_click_window(self, win_x, win_y):
        self.origin = self._window_origin()
        x, y = self._screen_xy(win_x, win_y)
        xtest.fake_input(self.disp, X.MotionNotify, x=x, y=y); self.disp.sync(); time.sleep(0.06)
        for _ in range(2):
            xtest.fake_input(self.disp, X.ButtonPress, 1); self.disp.sync()
            xtest.fake_input(self.disp, X.ButtonRelease, 1); self.disp.sync()
            time.sleep(0.06)
        time.sleep(0.15)

    def key(self, keysym_name, shift=False, ctrl=False):
        code = self.disp.keysym_to_keycode(XK.string_to_keysym(keysym_name))
        sc = self.disp.keysym_to_keycode(XK.string_to_keysym("Shift_L"))
        cc = self.disp.keysym_to_keycode(XK.string_to_keysym("Control_L"))
        if ctrl:
            xtest.fake_input(self.disp, X.KeyPress, cc); self.disp.sync()
        if shift:
            xtest.fake_input(self.disp, X.KeyPress, sc); self.disp.sync()
        xtest.fake_input(self.disp, X.KeyPress, code); self.disp.sync()
        xtest.fake_input(self.disp, X.KeyRelease, code); self.disp.sync()
        if shift:
            xtest.fake_input(self.disp, X.KeyRelease, sc); self.disp.sync()
        if ctrl:
            xtest.fake_input(self.disp, X.KeyRelease, cc); self.disp.sync()
        time.sleep(0.04)

    _CHARMAP = {" ": "space", "=": "equal", "[": "bracketleft", "]": "bracketright",
                "(": "parenleft", ")": "parenright", ";": "semicolon", "+": "plus",
                "*": "asterisk", "-": "minus", ".": "period", ",": "comma"}

    def type_text(self, text):
        for ch in text:
            if ch.isupper():
                self.key(ch.lower(), shift=True)
            else:
                self.key(self._CHARMAP.get(ch, ch))
        time.sleep(0.05)


# ---- assertion helpers ----------------------------------------------------

PASS, FAIL = [], []


def check(name, cond, detail=""):
    (PASS if cond else FAIL).append(name)
    mark = "PASS" if cond else "FAIL"
    print(f"  [{mark}] {name}{(' — ' + detail) if detail else ''}")


def summary_and_exit():
    print(f"\n{len(PASS)} passed, {len(FAIL)} failed")
    raise SystemExit(1 if FAIL else 0)
