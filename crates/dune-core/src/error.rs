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
}
