//! Engine errors. Deck-level problems surface as
//! `dune_netlist::Diagnostic`s during elaboration; these are the
//! failures of building or running a circuit.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Bad programmatic construction (duplicate name, zero-valued
    /// resistor, dangling reference, …).
    #[error("{0}")]
    Build(String),

    /// The deck uses a device the engine does not implement yet.
    #[error("`{name}`: {what} is not implemented yet")]
    Unsupported { name: String, what: String },

    /// The deck could not be elaborated into a circuit.
    #[error("{0}")]
    Elab(String),

    /// Singular MNA matrix (floating node, voltage-source loop, …).
    #[error("singular matrix: check node `{0}`")]
    Singular(String),

    /// `NIiter` gave up (`E_ITERLIM`).
    #[error("too many iterations without convergence")]
    IterLim,

    /// The transient step shrank below `delmin` (`E_TIMESTEP`).
    #[error("timestep too small at t = {time:.6e}")]
    Timestep { time: f64 },

    /// The transient attempted more internal timepoints than
    /// [`Options::max_tran_steps`](crate::Options::max_tran_steps)
    /// allows — Dune's guard against runaway breakpoint cascades
    /// (D-0022); neither the 3f5 nor ngspice enforces `ITL5`.
    #[error("transient exceeded {steps} internal steps at t = {time:.6e}")]
    StepLimit { steps: usize, time: f64 },

    /// `.pss`: the shooting iterations did not bring the period
    /// mismatch `x(t₀+T) − x(t₀)` within tolerance. `residuals` holds
    /// each iteration's mismatch in units of the tolerance
    /// ([`Pss::residuals`](crate::Pss::residuals)).
    #[error(
        "periodic steady state did not converge in {iterations} shooting iterations \
         (residual/tolerance per iteration: {})",
        list_e(.residuals)
    )]
    PssConvergence {
        iterations: usize,
        residuals: Vec<f64>,
    },

    /// An output selection named a vector the solution does not have.
    #[error("no such vector `{0}`")]
    UnknownVector(String),
}

/// `1.234e-3, 5.000e-1` — a residual history in an error message.
fn list_e(v: &[f64]) -> String {
    v.iter()
        .map(|r| format!("{r:.3e}"))
        .collect::<Vec<_>>()
        .join(", ")
}
