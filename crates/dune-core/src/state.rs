//! Device state vectors (`CKTstates[0..7]` in `cktdefs.h`): one
//! `f64` bank per past timepoint, rotated as time advances. Devices
//! own offsets handed out at build time, like `CKTnumStates`.

/// All state banks. Bank 0 is "now" (`CKTstate0`), bank 1 the
/// previous accepted point, and so on.
pub(crate) struct States {
    banks: Vec<Vec<f64>>,
}

pub(crate) const NUM_BANKS: usize = 8;

impl States {
    pub fn new(num: usize) -> Self {
        States {
            banks: (0..NUM_BANKS).map(|_| vec![0.0; num]).collect(),
        }
    }

    #[inline]
    pub fn get(&self, bank: usize, idx: usize) -> f64 {
        self.banks[bank][idx]
    }

    #[cfg(test)]
    pub fn set(&mut self, bank: usize, idx: usize, v: f64) {
        self.banks[bank][idx] = v;
    }

    /// The state rotation in `dctran.c`: the oldest useful bank is
    /// recycled as the new bank 0 (its stale contents are kept; the
    /// loads overwrite what they use).
    pub fn rotate(&mut self, maxord: usize) {
        self.banks[..maxord + 2].rotate_right(1);
    }

    /// Undo one [`States::rotate`] (a rejected TR-BDF2 stage): bank
    /// 1 becomes bank 0 again. The recycled bank goes back to the end
    /// holding the rejected stage's values, which nothing reads (the
    /// next rotation recycles it again).
    pub fn unrotate(&mut self, maxord: usize) {
        self.banks[..maxord + 2].rotate_left(1);
    }

    /// Bank-0 helpers for ngspice's dynamic gmin stepping
    /// (`cktop.c::dynamic_gmin` zeroes, saves and restores
    /// `CKTstate0`).
    pub fn zero0(&mut self) {
        self.banks[0].fill(0.0);
    }

    /// Slots per bank.
    pub fn len0(&self) -> usize {
        self.banks[0].len()
    }

    pub fn snapshot0(&self) -> Vec<f64> {
        self.banks[0].clone()
    }

    pub fn restore0(&mut self, saved: &[f64]) {
        self.banks[0].copy_from_slice(saved);
    }

    /// `memcpy(state2, state1)`, `memcpy(state3, state1)` after the
    /// first transient Newton (`optran.c`, `dctran.c`).
    pub fn copy1_to_23(&mut self) {
        let (head, rest) = self.banks.split_at_mut(2);
        rest[0].copy_from_slice(&head[1]);
        rest[1].copy_from_slice(&head[1]);
    }

    /// `bcopy(state0, state1, …)` after the transient operating
    /// point (`dctran.c`).
    pub fn copy01(&mut self) {
        let (b0, rest) = self.banks.split_first_mut().expect("banks");
        rest[0].copy_from_slice(b0);
    }
}

/// Mutable access to the state banks a load may touch: all of them
/// ([`States::view`]), or one worker's window of offsets
/// ([`States::split_views`]) under the parallel load.
///
/// Offsets stay global (a device indexes with the base it was given
/// at build time); the view subtracts its own start. An offset
/// outside the window is an out-of-bounds panic, never a silent read
/// of another worker's cells.
pub(crate) struct StateView<'a> {
    banks: [&'a mut [f64]; NUM_BANKS],
    base: usize,
}

impl StateView<'_> {
    #[inline]
    pub fn get(&self, bank: usize, idx: usize) -> f64 {
        self.banks[bank][idx - self.base]
    }

    #[inline]
    pub fn set(&mut self, bank: usize, idx: usize, v: f64) {
        self.banks[bank][idx - self.base] = v;
    }

    #[inline]
    pub fn add(&mut self, bank: usize, idx: usize, v: f64) {
        self.banks[bank][idx - self.base] += v;
    }
}

impl States {
    /// A view of every offset of every bank.
    pub fn view(&mut self) -> StateView<'_> {
        let mut it = self.banks.iter_mut();
        StateView {
            banks: std::array::from_fn(|_| it.next().expect("NUM_BANKS banks").as_mut_slice()),
            base: 0,
        }
    }

    /// Disjoint views of the windows `bounds[k]..bounds[k + 1]` of
    /// every bank (`bounds` nondecreasing, at most the bank length).
    /// The borrow checker sees the windows as separate slices, so the
    /// views can go to different threads.
    #[cfg_attr(not(feature = "parallel"), allow(dead_code))]
    pub fn split_views(&mut self, bounds: &[usize]) -> Vec<StateView<'_>> {
        let k = bounds.len().saturating_sub(1);
        let mut per: Vec<Vec<&mut [f64]>> = (0..k).map(|_| Vec::with_capacity(NUM_BANKS)).collect();
        for bank in self.banks.iter_mut() {
            let mut rest: &mut [f64] = &mut bank[bounds[0]..bounds[k]];
            for (w, slot) in bounds.windows(2).zip(per.iter_mut()) {
                let (head, tail) = std::mem::take(&mut rest).split_at_mut(w[1] - w[0]);
                slot.push(head);
                rest = tail;
            }
        }
        per.into_iter()
            .zip(bounds)
            .map(|(banks, &base)| StateView {
                banks: banks
                    .try_into()
                    .unwrap_or_else(|_| unreachable!("NUM_BANKS banks")),
                base,
            })
            .collect()
    }
}

/// Hands out state offsets during device allocation.
pub(crate) struct StateAlloc {
    next: usize,
}

impl StateAlloc {
    pub fn new() -> Self {
        StateAlloc { next: 0 }
    }

    pub fn alloc(&mut self, n: usize) -> usize {
        let base = self.next;
        self.next += n;
        base
    }

    pub fn total(&self) -> usize {
        self.next
    }
}
