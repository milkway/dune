//! The Newton–Raphson engine: `NIiter` (`ni/niiter.c`), `NIconvTest`
//! (`ni/niconv.c`) and the gmin/source-stepping driver `CKTop`
//! (`ckt/cktop.c`), DC subset.

use crate::circuit::{Built, VarKind, Vars};
use crate::devices::Device;
use crate::error::Error;
use crate::linalg::{LinalgError, LinearSolver};
use crate::matrix::{InitMode, LoadCtx, MatrixBuilder, Var};
use crate::options::Options;

pub(crate) struct Engine {
    pub vars: Vars,
    devices: Vec<Box<dyn Device>>,
    mb: MatrixBuilder,
    solver: Box<dyn LinearSolver>,
    values: Vec<f64>,
    /// `CKTrhs`: the RHS, overwritten by each solve with the new
    /// solution (1-based, `[0]` trash).
    rhs: Vec<f64>,
    /// `CKTrhsOld`: the previous solution; holds the final solution
    /// after success.
    pub sol: Vec<f64>,
    nodesets: Vec<(Var, f64)>,
    /// Diagonal slots of nodeset rows (`node->ptr` in `cktload.c`).
    nodeset_diag: Vec<crate::matrix::Slot>,
}

impl Engine {
    pub fn new(built: Built) -> Self {
        let Built {
            vars,
            mut devices,
            nodesets,
            ..
        } = built;
        let n = vars.len() - 1;
        let mut mb = MatrixBuilder::new(n);
        for d in &mut devices {
            d.alloc(&mut mb);
        }
        // Nodeset rows need their diagonal present.
        let nodeset_diag = nodesets.iter().map(|&(v, _)| mb.slot(v, v)).collect();
        let mut solver = crate::linalg::default_solver();
        solver.analyze(&mb.pattern());
        let num_slots = mb.num_slots();
        Engine {
            vars,
            devices,
            mb,
            solver,
            values: vec![0.0; num_slots],
            rhs: vec![0.0; n + 1],
            sol: vec![0.0; n + 1],
            nodesets,
            nodeset_diag,
        }
    }

    /// One `NIiter` call: Newton iterations until convergence or
    /// `maxiter`. `start` is the `INITF` state to begin in
    /// (`MODEINITJCT` on a fresh start, `MODEINITFLOAT` to continue
    /// from the previous solution during stepping).
    fn ni_iter(
        &mut self,
        opts: &Options,
        start: InitMode,
        maxiter: usize,
        diag_gmin: f64,
        srcfact: f64,
    ) -> Result<(), Error> {
        let mut mode = start;
        let mut iterno = 0usize;
        let mut ipass = false;
        let has_nodesets = !self.nodesets.is_empty();

        loop {
            // CKTload.
            self.values.fill(0.0);
            self.rhs.fill(0.0);
            let mut ctx = LoadCtx {
                values: &mut self.values,
                rhs: &mut self.rhs,
                sol: &self.sol,
                mode,
                gmin: opts.gmin,
                srcfact,
                noncon: false,
                reltol: opts.reltol,
                abstol: opts.abstol,
            };
            for d in &mut self.devices {
                d.load_dc(&mut ctx);
            }
            let mut noncon = ctx.noncon;

            // Nodesets hold their nodes during INITJCT/INITFIX
            // (`cktload.c`; we always use the row-replacement form,
            // see docs/dune-core.md).
            if has_nodesets && mode != InitMode::Float {
                for (k, &(var, value)) in self.nodesets.iter().enumerate() {
                    for s in self.mb.slots_in_row(var) {
                        self.values[s.0] = 0.0;
                    }
                    self.values[self.nodeset_diag[k].0] = 1.0;
                    self.rhs[var] = value;
                }
            }

            // Factor + solve; the solution lands in `rhs`.
            self.solver
                .factor(&self.values[1..], diag_gmin)
                .map_err(|e| self.map_singular(e))?;
            self.solver
                .solve_in_place(&mut self.rhs[1..])
                .map_err(|e| self.map_singular(e))?;

            iterno += 1;
            if iterno > maxiter {
                return Err(Error::IterLim);
            }
            if !noncon && iterno != 1 {
                noncon = !self.converged(opts);
            } else {
                noncon = true;
            }

            match mode {
                InitMode::Float => {
                    if has_nodesets {
                        // The extra all-devices-released pass after
                        // nodesets stop holding (`ipass` in the C).
                        if ipass {
                            noncon = true;
                        }
                        ipass = false;
                    }
                    if !noncon {
                        std::mem::swap(&mut self.sol, &mut self.rhs);
                        return Ok(());
                    }
                }
                InitMode::Jct => mode = InitMode::Fix,
                InitMode::Fix => {
                    if !noncon {
                        mode = InitMode::Float;
                    }
                    ipass = true;
                }
            }
            std::mem::swap(&mut self.sol, &mut self.rhs);
        }
    }

    /// `NIconvTest`: node voltages against `vntol`, branch currents
    /// against `abstol`, both with the `reltol` term.
    fn converged(&self, opts: &Options) -> bool {
        for i in 1..self.vars.len() {
            let new = self.rhs[i];
            let old = self.sol[i];
            let floor = match self.vars.kinds[i] {
                VarKind::Branch => opts.abstol,
                _ => opts.vntol,
            };
            let tol = opts.reltol * f64::max(new.abs(), old.abs()) + floor;
            if (new - old).abs() > tol {
                return false;
            }
        }
        true
    }

    fn map_singular(&self, e: LinalgError) -> Error {
        let LinalgError::Singular { row } = e;
        let name = self
            .vars
            .names
            .get(row + 1)
            .cloned()
            .unwrap_or_else(|| "?".into());
        Error::Singular(name)
    }

    /// `CKTop`: plain Newton first, then gmin stepping, then source
    /// stepping, exactly in the C's control flow.
    pub fn op(&mut self, opts: &Options) -> Result<(), Error> {
        let first = InitMode::Jct;
        let direct = if !opts.noopiter {
            self.ni_iter(opts, first, opts.itl1, 0.0, 1.0)
        } else {
            Err(Error::IterLim) // "go directly to gmin stepping"
        };
        if direct.is_ok() {
            return Ok(());
        }
        let mut last = direct;

        if opts.gminsteps > 1 {
            let mut diag_gmin = opts.gmin;
            for _ in 0..opts.gminsteps {
                diag_gmin *= 10.0;
            }
            let mut mode = first;
            for _ in 0..=opts.gminsteps {
                if self.ni_iter(opts, mode, opts.itl1, diag_gmin, 1.0).is_err() {
                    break; // "Gmin step failed"
                }
                diag_gmin /= 10.0;
                mode = InitMode::Float;
            }
            // The C always tries once more at gmin zero.
            last = self.ni_iter(opts, mode, opts.itl1, 0.0, 1.0);
            if last.is_ok() {
                return Ok(());
            }
        }

        if opts.srcsteps > 1 {
            let mut mode = first;
            for i in 0..=opts.srcsteps {
                let srcfact = i as f64 / opts.srcsteps as f64;
                let r = self.ni_iter(opts, mode, opts.itl1, 0.0, srcfact);
                mode = InitMode::Float;
                r?; // "source stepping failed"
            }
            return Ok(()); // "Source stepping completed"
        }

        last
    }
}
