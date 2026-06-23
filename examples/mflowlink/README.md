# mflowLink examples

## `quadrotor_cascade.mflow` — cascade flight controller

A mflowLink signal-flow rendering of the cascade flight controller in the
compiler's `examples/quadrotor/quadrotor_pid_mpc.m` (a quadrotor tracking a 3-D
figure-8). Open it in the IDE and press **▶ Simulate → Play** to watch it run:
the scopes stream live and the active block is haloed on the canvas as the
solver advances.

### Architecture (three control channels)

Each position axis is a textbook cascade — an outer position loop commanding a
tilt, an inner attitude loop tracking that tilt, and a linearized plant:

```
figure-8 ref ─▶ (ref − pos − 0.8·vel) ─▶ MPC ─▶ tilt_cmd
                                                   │
                          ┌──── θ feedback ◀───────┤
                          ▼                         ▼
        tilt_cmd − θ ─▶ PID ─▶ 1/I ─▶ ∫ ─▶ ∫ ─▶ θ ─▶ ×g ─▶ ∫ ─▶ ∫ ─▶ pos ──┐
                                                                            │
                          └──────────────── pos / vel feedback ◀───────────┘
```

* **x / pitch** — `2·sin(0.6 t)` reference (the wide lobe of the figure-8).
* **y / roll** — `1·sin(1.2 t)` reference (the fast lobe). The command gain is
  negated to mirror `y_ddot = −g·φ`, so the loop stays negative-feedback.
* **z / altitude** — a PID holds `z = 1 m`.

### How it maps to the `.m`

| `.m` element | Diagram block |
|---|---|
| `mpc()` / `mpcmove()` outer loop | `signal_mpc_move` (per axis) |
| `pid()` inner loops (φ, θ, z) | `signal_sum` (error) + `signal_pid` |
| `x_ddot = g·θ`, `θ_ddot = u/I` | `signal_gain` + `signal_integrator` pairs |
| figure-8 reference | `signal_sine` |
| scope logging | `signal_scope` (reference + actual per axis) |

### Simplifications (compiler/runtime constraints)

The `.m` runs a full 6-DOF **nonlinear** plant with inline RK4 and a real QP
**MPC**. The simulator's `signal_mpc_move` is a static-gain approximation
(`u = gain·(r − ym)`), and a 12-state nonlinear plant isn't expressible in
blocks, so this diagram uses:

* the **linearized** outer plant (`x_ddot = g·θ`, `y_ddot = −g·φ`) the `.m`'s MPC
  itself is built on, plus double-integrator attitude dynamics;
* a **P + velocity-lead** outer loop in place of the QP MPC (a velocity term
  gives the damping the QP horizon would otherwise provide);
* x, y, z channels (yaw holds 0 and is omitted).

It is a faithful rendering of the controller *structure* and runs/animates
end-to-end; it is not a bit-exact reproduction of the nonlinear closed loop.

`quadrotor_cascade.gen.py` regenerates the `.mflow` (edit the gains/topology
there rather than hand-editing the JSON).
