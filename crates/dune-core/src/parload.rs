//! The parallel device load (issue #9, lever 1; feature `parallel`).
//!
//! `CKTload` walks the devices in order and each one adds its stamps
//! into the matrix and the RHS. Device evaluation is independent per
//! device — what is *not* is the floating-point summation: a matrix
//! position shared by several devices ends up as
//! `((0 + a) + b) + c` in device order, and any other grouping (per
//! worker partial sums merged at the end) changes ulps.
//!
//! So the workers do not sum. The devices are split into contiguous
//! runs ("chunks") in device order; each worker evaluates its chunk
//! into a private [`StampLog`] — the list of `(position, value)`
//! stamps in the order the chunk's devices made them — and the logs
//! are then replayed into the matrix chunk by chunk, in chunk order,
//! on the calling thread. The replay performs exactly the additions
//! of the sequential loop, in the same order per position: the
//! result is bit-identical to the sequential load, for any number of
//! threads, any chunking and any scheduling. The replay costs one
//! add per stamp — what the sequential loop pays anyway — while the
//! device equations (thousands of flops per BSIM3 instance against
//! ~30 stamps) run in parallel.
//!
//! The rest of what a load touches is made disjoint by construction:
//!
//! - device fields: each worker gets its own `&mut` slice of the
//!   device list;
//! - state banks: each worker gets a [`StateView`] of only the state
//!   offsets its devices allocated (offsets are handed out in device
//!   order, so a chunk's offsets are one contiguous window); an access
//!   outside the window panics instead of racing;
//! - the matrix/RHS: [`LoadCtx`]'s sink is write-only, so no device
//!   can read what another stamped;
//! - `noncon` is an OR and `fault` keeps the last device's, merged in
//!   chunk order — the sequential semantics.
//!
//! A device whose load reaches outside its own window (the mutual
//! inductor adds into both inductors' flux states) is not
//! [`par_load`](crate::devices::Device::par_load): it splits the
//! device list, loading in place between the parallel runs before and
//! after it, which keeps its position in the sequential order.

use std::ops::Range;

use crate::devices::Device;
use crate::matrix::{LoadEnv, StampLog, StampSink};
use crate::state::{StateView, States};

/// Chunks per worker thread: more chunks than threads lets rayon's
/// work stealing even out devices of unequal cost.
const CHUNKS_PER_THREAD: usize = 4;

/// One step of the load plan, in device order.
#[derive(Debug, PartialEq)]
enum Step {
    /// A device loaded in place on the calling thread.
    InPlace(usize),
    /// A run of parallel-safe devices split into contiguous chunks
    /// (covering the run, in order).
    Parallel(Vec<Range<usize>>),
}

/// The thread pool, the plan and the per-chunk logs of one engine.
pub(crate) struct ParLoad {
    /// The `load_threads` the pool was built for.
    requested: usize,
    pool: rayon::ThreadPool,
    plan: Vec<Step>,
    logs: Vec<StampLog>,
}

impl ParLoad {
    /// A pool of `threads` workers (`0`: one per core) and the plan
    /// for `devices`; `None` if the pool cannot be built (the load
    /// then stays sequential, which gives the same result).
    pub fn new(
        threads: usize,
        devices: &[Box<dyn Device>],
        ranges: &[(usize, usize)],
    ) -> Option<Self> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .ok()?;
        let chunks = pool.current_num_threads() * CHUNKS_PER_THREAD;
        let safe: Vec<bool> = devices.iter().map(|d| d.par_load()).collect();
        let plan = plan(&safe, ranges, chunks);
        let nlogs = plan
            .iter()
            .map(|s| match s {
                Step::Parallel(c) => c.len(),
                Step::InPlace(_) => 0,
            })
            .max()
            .unwrap_or(0);
        Some(ParLoad {
            requested: threads,
            pool,
            plan,
            logs: (0..nlogs).map(|_| StampLog::default()).collect(),
        })
    }

    pub fn requested(&self) -> usize {
        self.requested
    }
}

/// Split the device list into in-place devices and parallel runs,
/// each run into at most `chunks` contiguous chunks of about equal
/// weight (`1 +` the device's state count, a cheap proxy for its load
/// cost). With at least as many chunks as devices, every device is its
/// own chunk — each worker window is then exactly one device's.
fn plan(safe: &[bool], ranges: &[(usize, usize)], chunks: usize) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut i = 0;
    while i < safe.len() {
        if !safe[i] {
            steps.push(Step::InPlace(i));
            i += 1;
            continue;
        }
        let start = i;
        while i < safe.len() && safe[i] {
            i += 1;
        }
        if i - start == 1 {
            steps.push(Step::InPlace(start));
        } else {
            steps.push(Step::Parallel(split(start..i, ranges, chunks.max(1))));
        }
    }
    steps
}

/// Contiguous chunks of `run` of about equal weight.
fn split(run: Range<usize>, ranges: &[(usize, usize)], chunks: usize) -> Vec<Range<usize>> {
    if run.len() <= chunks {
        return run.map(|d| d..d + 1).collect();
    }
    let weight = |d: usize| 1 + ranges[d].1 - ranges[d].0;
    let total: usize = run.clone().map(weight).sum();
    let mut out = Vec::with_capacity(chunks);
    let (mut lo, mut acc) = (run.start, 0usize);
    for d in run.clone() {
        acc += weight(d);
        // Close the chunk once it reaches its share of the total.
        if acc * chunks >= total * (out.len() + 1) && out.len() + 1 < chunks {
            out.push(lo..d + 1);
            lo = d + 1;
        }
    }
    if lo < run.end {
        out.push(lo..run.end);
    }
    out
}

/// What one worker gets: its devices, its state window, its log, and
/// the flags its loads raised.
struct Job<'a> {
    devices: &'a mut [Box<dyn Device>],
    states: Option<StateView<'a>>,
    log: &'a mut StampLog,
    noncon: bool,
    fault: Option<&'static str>,
}

/// One full device load (`DEVload` of every device, in device order)
/// through the plan, into `values`/`rhs` (zeroed by the caller).
/// `env` builds each device context from a sink and a state view;
/// returns `(noncon, fault)` as the sequential loop would leave them.
pub(crate) fn load(
    par: &mut ParLoad,
    devices: &mut [Box<dyn Device>],
    ranges: &[(usize, usize)],
    states: &mut States,
    values: &mut [f64],
    rhs: &mut [f64],
    env: &LoadEnv<'_>,
) -> (bool, Option<&'static str>) {
    let mut noncon = false;
    let mut fault = None;
    for step in &par.plan {
        match step {
            Step::InPlace(d) => {
                let sink = StampSink::Direct {
                    values: &mut *values,
                    rhs: &mut *rhs,
                };
                let mut c = env.ctx(sink, states.view());
                devices[*d].load(&mut c);
                noncon |= c.noncon;
                fault = c.fault.or(fault);
            }
            Step::Parallel(chunks) => {
                let (lo, hi) = (chunks[0].start, chunks[chunks.len() - 1].end);
                // The chunks' state windows: contiguous, since offsets
                // are allocated in device order.
                let mut bounds: Vec<usize> = chunks.iter().map(|c| ranges[c.start].0).collect();
                bounds.push(ranges[hi - 1].1);
                let views = states.split_views(&bounds);
                let mut rest = &mut devices[lo..hi];
                let mut jobs: Vec<Job> = Vec::with_capacity(chunks.len());
                for ((c, view), log) in chunks.iter().zip(views).zip(par.logs.iter_mut()) {
                    let (mine, tail) = std::mem::take(&mut rest).split_at_mut(c.len());
                    rest = tail;
                    log.clear();
                    jobs.push(Job {
                        devices: mine,
                        states: Some(view),
                        log,
                        noncon: false,
                        fault: None,
                    });
                }
                par.pool.install(|| {
                    use rayon::prelude::*;
                    jobs.par_iter_mut().for_each(|job| {
                        let view = job.states.take().expect("one load per job");
                        let mut c = env.ctx(StampSink::Record(&mut *job.log), view);
                        for d in job.devices.iter_mut() {
                            d.load(&mut c);
                        }
                        job.noncon = c.noncon;
                        job.fault = c.fault;
                    });
                });
                // The deterministic reduction: chunk order = device
                // order, whatever order the workers finished in.
                for job in &jobs {
                    job.log.replay(values, rhs);
                    noncon |= job.noncon;
                    fault = job.fault.or(fault);
                }
            }
        }
    }
    (noncon, fault)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_splits_at_unsafe_devices() {
        let safe = [true, true, false, true, true, true, false, true];
        let ranges: Vec<(usize, usize)> = (0..8).map(|i| (i, i)).collect();
        let p = plan(&safe, &ranges, 2);
        assert_eq!(
            p,
            vec![
                Step::Parallel(vec![0..1, 1..2]),
                Step::InPlace(2),
                Step::Parallel(vec![3..5, 5..6]),
                Step::InPlace(6),
                Step::InPlace(7),
            ]
        );
    }

    #[test]
    fn split_covers_the_run_in_order() {
        // Weights 1 + states: one heavy device among light ones.
        let mut ranges = Vec::new();
        let mut s = 0;
        for d in 0..40 {
            let n = if d % 7 == 0 { 17 } else { 0 };
            ranges.push((s, s + n));
            s += n;
        }
        for chunks in [1, 2, 3, 8, 39, 40, 100] {
            let c = split(0..40, &ranges, chunks);
            assert!(c.len() <= chunks.max(1));
            assert_eq!(c[0].start, 0);
            assert_eq!(c[c.len() - 1].end, 40);
            for w in c.windows(2) {
                assert_eq!(w[0].end, w[1].start);
                assert!(!w[0].is_empty());
            }
        }
    }
}
