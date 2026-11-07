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

    #[inline]
    pub fn set(&mut self, bank: usize, idx: usize, v: f64) {
        self.banks[bank][idx] = v;
    }

    #[inline]
    pub fn add(&mut self, bank: usize, idx: usize, v: f64) {
        self.banks[bank][idx] += v;
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

    pub fn snapshot0(&self) -> Vec<f64> {
        self.banks[0].clone()
    }

    pub fn restore0(&mut self, saved: &[f64]) {
        self.banks[0].copy_from_slice(saved);
    }

    /// `bcopy(state0, state1, …)` after the transient operating
    /// point (`dctran.c`).
    pub fn copy01(&mut self) {
        let (b0, rest) = self.banks.split_first_mut().expect("banks");
        rest[0].copy_from_slice(b0);
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
