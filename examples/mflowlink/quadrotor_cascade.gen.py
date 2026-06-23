#!/usr/bin/env python3
"""Generate a mflowLink cascade-controller diagram for the quadrotor example.

Per axis (x/pitch, y/roll): figure-8 sine reference → outer MPC (static-gain P,
with velocity lead for damping) → tilt command → inner PID on attitude →
torque/inertia → double-integrator attitude → ×g → double-integrator position →
fed back. A mux overlays reference vs actual on a scope. Altitude (z) is a
PID → 1/m → double integrator holding 1 m.
"""
import json, sys

PORTS = {
    "signal_sine": ([], ["out"]), "signal_constant": ([], ["out"]),
    "signal_mpc_move": (["ym", "r"], ["out"]),
    "signal_sum": (["in1", "in2"], ["out"]),
    "signal_pid": (["in"], ["out"]), "signal_gain": (["in"], ["out"]),
    "signal_integrator": (["in"], ["out"]),
    "signal_mux": (["in1", "in2"], ["out"]),
    "signal_scope": (["in"], []),
}

nodes, edges, _eid = [], [], [0]

def N(nid, kind, params, x, y):
    ins, outs = PORTS[kind]
    nodes.append({
        "id": nid, "kind": kind,
        "label": params.pop("_label", ""),
        "data": {"params": params},
        "ports": {"in": [{"id": p} for p in ins], "out": [{"id": p} for p in outs]},
        "ui": {"position": {"x": x, "y": y}},
    })

def E(src, sp, dst, dp):
    _eid[0] += 1
    edges.append({"id": f"e{_eid[0]}", "kind": "data",
                  "from": {"node": src, "port": sp}, "to": {"node": dst, "port": dp}})

def axis(prefix, amp, freq, g_sign, y0):
    """One position channel. g_sign = +1 (pitch→x) or -1 (roll→y)."""
    g = 9.81 * g_sign
    col = lambda c: 60 + c * 150
    # reference
    N(f"{prefix}ref", "signal_sine", {"amplitude": amp, "frequency": freq}, col(0), y0)
    # velocity lead: ym = pos + 0.8*vel  (PD outer loop for damping)
    N(f"{prefix}lead", "signal_gain", {"gain": 0.8}, col(1), y0 + 120)
    N(f"{prefix}ym", "signal_sum", {"signs": "++"}, col(2), y0 + 80)
    # outer MPC (static-gain): tilt_cmd = 0.4*g_sign*(ref - ym). The g_sign
    # factor mirrors the real MPC's B-matrix sign (y_ddot = -g*phi), so the
    # closed loop is negative feedback on both axes.
    N(f"{prefix}mpc", "signal_mpc_move", {"gain": 0.4 * g_sign}, col(3), y0)
    # inner attitude loop: err = tilt_cmd - angle, PID -> torque
    N(f"{prefix}aerr", "signal_sum", {"signs": "+-"}, col(4), y0)
    N(f"{prefix}pid", "signal_pid", {"Kp": 6.0, "Ki": 0.0, "Kd": 0.45, "N": 100.0}, col(5), y0)
    N(f"{prefix}invI", "signal_gain", {"gain": 100.0}, col(6), y0)          # 1/Iy
    N(f"{prefix}thd", "signal_integrator", {"initialCondition": 0.0}, col(7), y0)   # angle rate
    N(f"{prefix}th", "signal_integrator", {"initialCondition": 0.0}, col(8), y0)    # angle
    N(f"{prefix}g", "signal_gain", {"gain": g}, col(9), y0)                 # x_ddot = g*angle
    N(f"{prefix}vd", "signal_integrator", {"initialCondition": amp * freq}, col(10), y0)  # velocity (trim)
    N(f"{prefix}pos", "signal_integrator", {"initialCondition": 0.0}, col(11), y0)   # position
    # log reference + actual position as separate scopes (the IDE scope panel
    # overlays all logged signals on one set of axes)
    N(f"{prefix}_ref_sc", "signal_scope", {"yMin": -3.0, "yMax": 3.0, "title": f"{prefix} reference"}, col(12), y0 - 80)
    N(f"{prefix}_act_sc", "signal_scope", {"yMin": -3.0, "yMax": 3.0, "title": f"{prefix} actual"}, col(12), y0 + 80)
    # forward path
    E(f"{prefix}ref", "out", f"{prefix}mpc", "r")
    E(f"{prefix}ym", "out", f"{prefix}mpc", "ym")
    E(f"{prefix}mpc", "out", f"{prefix}aerr", "in1")
    E(f"{prefix}aerr", "out", f"{prefix}pid", "in")
    E(f"{prefix}pid", "out", f"{prefix}invI", "in")
    E(f"{prefix}invI", "out", f"{prefix}thd", "in")
    E(f"{prefix}thd", "out", f"{prefix}th", "in")
    E(f"{prefix}th", "out", f"{prefix}g", "in")
    E(f"{prefix}g", "out", f"{prefix}vd", "in")
    E(f"{prefix}vd", "out", f"{prefix}pos", "in")
    # feedback
    E(f"{prefix}th", "out", f"{prefix}aerr", "in2")       # angle -> inner error
    E(f"{prefix}pos", "out", f"{prefix}ym", "in1")        # position -> outer
    E(f"{prefix}vd", "out", f"{prefix}lead", "in")        # velocity -> lead
    E(f"{prefix}lead", "out", f"{prefix}ym", "in2")
    # logging
    E(f"{prefix}ref", "out", f"{prefix}_ref_sc", "in")
    E(f"{prefix}pos", "out", f"{prefix}_act_sc", "in")

axis("x", 2.0, 0.6, +1, 60)
axis("y", 1.0, 1.2, -1, 520)

# Altitude hold: z PID -> 1/m -> double integrator, holding z = 1 m.
def altitude(y0):
    col = lambda c: 60 + c * 150
    N("zref", "signal_constant", {"value": 1.0}, col(0), y0)
    N("zerr", "signal_sum", {"signs": "+-"}, col(3), y0)
    N("zpid", "signal_pid", {"Kp": 8.0, "Ki": 2.0, "Kd": 4.0, "N": 50.0}, col(5), y0)
    N("zinvm", "signal_gain", {"gain": 1.0}, col(6), y0)          # 1/m
    N("zvd", "signal_integrator", {"initialCondition": 0.0}, col(7), y0)   # z rate
    N("zpos", "signal_integrator", {"initialCondition": 1.0}, col(8), y0)  # z (start 1 m)
    N("z_ref_sc", "signal_scope", {"yMin": 0.0, "yMax": 2.0, "title": "z reference"}, col(10), y0 - 80)
    N("z_act_sc", "signal_scope", {"yMin": 0.0, "yMax": 2.0, "title": "z actual"}, col(10), y0 + 80)
    E("zref", "out", "zerr", "in1")
    E("zerr", "out", "zpid", "in")
    E("zpid", "out", "zinvm", "in")
    E("zinvm", "out", "zvd", "in")
    E("zvd", "out", "zpos", "in")
    E("zpos", "out", "zerr", "in2")
    E("zref", "out", "z_ref_sc", "in")
    E("zpos", "out", "z_act_sc", "in")

altitude(980)

doc = {
    "schema": "matforge.flowchart", "version": "0.1.0", "entry": "quad",
    "settings": {"kind": "signal_flow", "solver": {
        "type": "fixed_step", "algorithm": "ode4",
        "startTime": 0.0, "stopTime": 10.0, "maxStep": "0.01"}},
    "flows": [{
        "id": "quad", "kind": "program", "name": "quad",
        "signature": {"inputs": [], "outputs": []},
        "nodes": nodes, "edges": edges,
    }],
}
out = sys.argv[1] if len(sys.argv) > 1 else "/tmp/quad.mflow"
json.dump(doc, open(out, "w"), indent=2)
print(f"wrote {out}: {len(nodes)} nodes, {len(edges)} edges")
