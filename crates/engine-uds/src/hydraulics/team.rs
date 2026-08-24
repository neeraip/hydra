//! The §6.4 worker team: a persistent pool that stays hot between jobs.
//!
//! The channel phase issues on the order of a million short parallel
//! regions per large run, each tens of microseconds of work. A pool that
//! parks its workers between regions pays a wake for every one of them —
//! measured on a 1,044-channel network, that overhead exceeded the work
//! itself and made width slower than serial. This team spins briefly
//! before parking, so a worker that finishes a region is still hot when
//! the next one is published a few microseconds later; it parks only
//! across genuinely long gaps, and the publisher unparks it.
//!
//! Correctness protocol, in one place. A job is published by writing the
//! job slot under its mutex, resetting `done`, opening the claim cursor
//! under a fresh generation, and finally advancing `seq` as the doorbell.
//! Workers wait on `seq`, then copy the slot under the mutex — the copy,
//! not the slot, is what they work from — and claim chunks by CAS on the
//! generation-tagged cursor. A worker that overslept a whole job either
//! copies the *current* job (and correctly works on it), or holds a stale
//! copy whose generation no longer matches the cursor: its claim fails
//! before it dereferences anything, so the dangled closure behind a dead
//! job is never touched. The publisher returns only when `done` reaches
//! the item count, which bounds every borrow the slot erases; the
//! Release adds on `done` against its Acquire wait are what make the
//! workers' writes visible to the caller.
//!
//! Only compiled with the `threads` feature; never part of a wasm build.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle, Thread};

/// Spins a waiting thread performs before yielding or parking. At about a
/// nanosecond per spin this covers the tens-of-microseconds serial join
/// between two channel-phase regions — exactly the gap that must not
/// cost a wake.
const SPINS_BEFORE_PARK: u32 = 20_000;

/// Items a worker claims per grab: small enough to balance channels of
/// very different cost (a kept channel is nearly free, §6.4), large
/// enough to amortise the claim.
const CHUNK: u64 = 32;

/// The published job. `f` is type-erased; its validity is bounded by the
/// protocol above, and a copy whose generation has lapsed is never
/// dereferenced.
#[derive(Clone, Copy)]
struct Job {
    gen: u32,
    f: *const (dyn Fn(usize) + Sync),
    n: usize,
    /// How the job distributes: chunk-claimed indices (`run`), or one
    /// call per team slot (`run_spmd`), where the closure synchronises
    /// itself with a [`Barrier`].
    spmd: bool,
}

// SAFETY: `Job` crosses threads only as a mutex-guarded copy, and the
// closure it points to is `Sync` with its lifetime bounded by the
// publisher's completion wait.
unsafe impl Send for Job {}

fn idle(_: usize) {}

/// A raw pointer the §6.4 map's closure may carry across the team.
///
/// SAFETY (of the impls): the team calls the closure with each index at
/// most once, so writes through the pointer are per-index disjoint, and
/// `Team::run` returns only after every write completes — the pointee
/// strictly outlives all access.
pub struct SendPtr<T>(*mut T);
unsafe impl<T> Sync for SendPtr<T> {}
unsafe impl<T> Send for SendPtr<T> {}

impl<T> SendPtr<T> {
    pub fn new(p: *mut T) -> SendPtr<T> {
        SendPtr(p)
    }

    /// Accessor rather than a public field: closure capture is by whole
    /// struct through the method's receiver, so the `Sync` promise above
    /// travels with the pointer instead of being disjointed away.
    pub fn get(&self) -> *mut T {
        self.0
    }
}

/// A sense-reversing spin barrier for the slots of one SPMD region.
///
/// Each slot carries a local sense flag (start `false`) and passes it to
/// every `wait`; the barrier flips its shared sense once the last slot
/// arrives, which releases the spinners and leaves the barrier armed for
/// the next phase with no reset step. The counter's read-modify-writes
/// form a release sequence, so everything a slot wrote before arriving
/// is visible to every slot after release — the barrier is the region's
/// only ordering, and it is enough.
///
/// Waits spin (with a yield fallback for oversubscribed hosts) and never
/// park: a barrier separates phases microseconds apart, inside a region
/// whose publisher already holds every participant awake.
pub struct Barrier {
    width: usize,
    count: AtomicUsize,
    sense: AtomicBool,
}

impl Barrier {
    pub fn new(width: usize) -> Barrier {
        Barrier {
            width,
            count: AtomicUsize::new(0),
            sense: AtomicBool::new(false),
        }
    }

    /// Arrive and wait for the rest of the team. `sense` is this slot's
    /// local flag: `false` before the first wait, handed back unchanged
    /// between waits, never shared between slots.
    pub fn wait(&self, sense: &mut bool) {
        *sense = !*sense;
        if self.count.fetch_add(1, Ordering::AcqRel) + 1 == self.width {
            self.count.store(0, Ordering::Relaxed);
            self.sense.store(*sense, Ordering::Release);
        } else {
            let mut spins = 0u32;
            while self.sense.load(Ordering::Acquire) != *sense {
                spins += 1;
                if spins < SPINS_BEFORE_PARK {
                    std::hint::spin_loop();
                } else {
                    thread::yield_now();
                }
            }
        }
    }
}

const fn pack(gen: u32, idx: u32) -> u64 {
    ((gen as u64) << 32) | idx as u64
}

struct Shared {
    /// Doorbell: the current job's generation, advanced last on publish.
    seq: AtomicU64,
    job: Mutex<Job>,
    /// Claim cursor: generation in the high half, next unclaimed index in
    /// the low. A claim CAS requires the generation to match the
    /// claimant's job copy, which is what makes stale copies inert.
    cursor: AtomicU64,
    /// Items completed for the current job.
    done: AtomicUsize,
    stop: AtomicBool,
}

pub struct Team {
    shared: Arc<Shared>,
    handles: Vec<JoinHandle<()>>,
    parked: Vec<Thread>,
    gen: u32,
}

impl Team {
    /// Spawn a team of `width - 1` workers (the caller is the width'th
    /// member). `None` below width 2, where a team is pure overhead.
    pub fn new(width: usize) -> Option<Team> {
        if width < 2 {
            return None;
        }
        let shared = Arc::new(Shared {
            seq: AtomicU64::new(0),
            job: Mutex::new(Job {
                gen: 0,
                f: &idle,
                n: 0,
                spmd: false,
            }),
            cursor: AtomicU64::new(pack(0, u32::MAX)),
            done: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
        });
        let mut handles = Vec::with_capacity(width - 1);
        for slot in 1..width {
            let sh = Arc::clone(&shared);
            handles.push(thread::spawn(move || worker(&sh, slot)));
        }
        let parked = handles.iter().map(|h| h.thread().clone()).collect();
        Some(Team {
            shared,
            handles,
            parked,
            gen: 0,
        })
    }

    /// The team's width: the caller plus its workers.
    pub fn width(&self) -> usize {
        self.handles.len() + 1
    }

    /// Run `f(slot)` exactly once per team slot — the caller as slot 0,
    /// each worker as its own slot — returning once every call is
    /// complete. The closure is the whole parallel region: it
    /// synchronises its phases itself with a [`Barrier`] of this team's
    /// width, and every slot must reach every barrier the closure waits
    /// on, or the region deadlocks.
    ///
    /// This exists because a region dispatched per phase costs a
    /// publish-and-complete round per phase — measured break-even near
    /// five microseconds of work — where a barrier between phases inside
    /// one region costs a fraction of one. An iteration whose phases are
    /// each a few microseconds only gains from width when the whole
    /// iteration is one region.
    pub fn run_spmd<F: Fn(usize) + Sync>(&mut self, f: F) {
        let width = self.width();
        self.gen = self.gen.wrapping_add(1);
        let gen = self.gen;
        let sh = &*self.shared;
        {
            // SAFETY: as in `run` — the transmute erases the borrow's
            // lifetime only, and this function returns only after every
            // slot has completed its call.
            let erased: *const (dyn Fn(usize) + Sync) = unsafe {
                std::mem::transmute::<*const (dyn Fn(usize) + Sync), *const (dyn Fn(usize) + Sync)>(
                    &f as &(dyn Fn(usize) + Sync) as *const _,
                )
            };
            *sh.job.lock().expect("team poisoned") = Job {
                gen,
                f: erased,
                n: width,
                spmd: true,
            };
        }
        sh.done.store(0, Ordering::Relaxed);
        // No cursor: slots are assigned, not claimed. Lapse the cursor's
        // generation so a stale chunk worker cannot claim into this job.
        sh.cursor.store(pack(gen, u32::MAX), Ordering::Release);
        sh.seq.store(u64::from(gen), Ordering::Release);
        for t in &self.parked {
            t.unpark();
        }
        f(0);
        sh.done.fetch_add(1, Ordering::Release);
        let mut spins = 0u32;
        while sh.done.load(Ordering::Acquire) < width {
            spins += 1;
            if spins < SPINS_BEFORE_PARK {
                std::hint::spin_loop();
            } else {
                thread::yield_now();
            }
        }
    }

    /// Run `f(i)` for every `i in 0..n` across the team, the caller
    /// included, returning once every item is complete.
    ///
    /// `f` may be called concurrently from every team thread; what it
    /// writes must be per-index disjoint, which is the §6.4 map's shape.
    pub fn run<F: Fn(usize) + Sync>(&mut self, n: usize, f: F) {
        if n == 0 {
            return;
        }
        u32::try_from(n).expect("§6.4 maps are far below 2^32 items");
        self.gen = self.gen.wrapping_add(1);
        let gen = self.gen;
        let sh = &*self.shared;
        {
            // SAFETY: the transmute only erases the borrow's lifetime
            // (identical layout either side); the deref side of the
            // bargain is honoured because this function returns only
            // after `done == n`, a worker adds to `done` strictly after
            // its last call into `f`, and a copy whose generation lapsed
            // is never dereferenced again.
            let erased: *const (dyn Fn(usize) + Sync) = unsafe {
                std::mem::transmute::<*const (dyn Fn(usize) + Sync), *const (dyn Fn(usize) + Sync)>(
                    &f as &(dyn Fn(usize) + Sync) as *const _,
                )
            };
            *sh.job.lock().expect("team poisoned") = Job {
                gen,
                f: erased,
                n,
                spmd: false,
            };
        }
        sh.done.store(0, Ordering::Relaxed);
        sh.cursor.store(pack(gen, 0), Ordering::Release);
        sh.seq.store(u64::from(gen), Ordering::Release);
        for t in &self.parked {
            t.unpark();
        }
        // The caller is a team member too.
        work_chunks(sh, gen, &f, n);
        // Completion barrier; Acquire pairs with the workers' Release
        // adds, publishing everything `f` wrote back to this thread.
        let mut spins = 0u32;
        while sh.done.load(Ordering::Acquire) < n {
            spins += 1;
            if spins < SPINS_BEFORE_PARK {
                std::hint::spin_loop();
            } else {
                thread::yield_now();
            }
        }
    }
}

/// Claim and process chunks for job `gen`. Returns when the cursor is
/// exhausted or its generation no longer matches.
fn work_chunks(sh: &Shared, gen: u32, f: &(dyn Fn(usize) + Sync), n: usize) {
    loop {
        let cur = sh.cursor.load(Ordering::Acquire);
        if (cur >> 32) as u32 != gen {
            return;
        }
        let idx = (cur & u32::MAX as u64) as usize;
        if idx >= n {
            return;
        }
        let take = CHUNK.min((n - idx) as u64);
        let next = pack(gen, (idx as u64 + take) as u32);
        if sh
            .cursor
            .compare_exchange_weak(cur, next, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
        {
            continue;
        }
        for i in idx..idx + take as usize {
            f(i);
        }
        sh.done.fetch_add(take as usize, Ordering::Release);
    }
}

fn worker(sh: &Shared, slot: usize) {
    let mut seen = 0u64;
    loop {
        // Wait for the doorbell: spin hot first, park across long gaps.
        let mut spins = 0u32;
        loop {
            let s = sh.seq.load(Ordering::Acquire);
            if s != seen {
                // `seen` syncs to the job copy below, not to `s`: the
                // copy may already be a later generation than the
                // doorbell that woke us.
                break;
            }
            if sh.stop.load(Ordering::Relaxed) {
                return;
            }
            spins += 1;
            if spins < SPINS_BEFORE_PARK {
                std::hint::spin_loop();
            } else {
                // Indefinite: the publisher and Drop both unpark, and the
                // park token makes an unpark-before-park race harmless. A
                // timeout here would have every idle worker polling
                // through the run's serial phases.
                thread::park();
            }
        }
        // Work from a copy; the mutex orders it against the publisher's
        // write, and the generation-tagged claims make a stale copy
        // inert without ever dereferencing it.
        let job = *sh.job.lock().expect("team poisoned");
        // The copy is whatever job is current, which may already be a
        // generation past the doorbell value that woke us (the publisher
        // of a chunks job returns without needing every worker). Sync
        // `seen` to the copy: without this, a worker that executed job
        // k+1 through an early break meets k+1 at the doorbell again and
        // runs it twice — for an SPMD job, a double execution and a
        // double completion count.
        seen = u64::from(job.gen);
        if job.spmd {
            // An SPMD job is current by construction: the publisher's
            // completion wait spans every slot's call, so a worker that
            // reached here through this generation's doorbell holds a
            // live closure. Oversleeping a generation entirely is
            // impossible while the publisher blocks on `done`.
            if slot < job.n {
                // SAFETY: the publisher waits for `done == n`, which
                // this slot's add below is part of, so the closure
                // outlives the call.
                let f = unsafe { &*job.f };
                f(slot);
            }
            sh.done.fetch_add(1, Ordering::Release);
            continue;
        }
        // SAFETY: dereferenced only inside claims whose generation
        // matches this copy, which the protocol bounds to the closure's
        // lifetime (see `Team::run`).
        let f = unsafe { &*job.f };
        work_chunks(sh, job.gen, f, job.n);
    }
}

impl Drop for Team {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        for t in &self.parked {
            t.unpark();
        }
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    /// Every index is visited exactly once, at every size that exercises
    /// chunk boundaries.
    #[test]
    fn a_job_visits_every_index_exactly_once() {
        let mut team = Team::new(4).expect("width 4");
        for n in [0usize, 1, 31, 32, 33, 1000] {
            let hits: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();
            team.run(n, |i| {
                hits[i].fetch_add(1, Ordering::Relaxed);
            });
            assert!(
                hits.iter().all(|h| h.load(Ordering::Relaxed) == 1),
                "n = {n}"
            );
        }
    }

    /// Back-to-back jobs on one hot team — the shape the channel phase
    /// drives it with, including closures with distinct captures.
    #[test]
    fn back_to_back_jobs_all_complete() {
        let mut team = Team::new(3).expect("width 3");
        let total = AtomicU32::new(0);
        for round in 0..10_000u32 {
            let bump = 1 + (round % 3);
            team.run(64, |_| {
                total.fetch_add(bump, Ordering::Relaxed);
            });
        }
        assert_eq!(total.load(Ordering::Relaxed), 64 * (10_000 + 9999));
    }

    /// Width below two is refused: a team of one is pure overhead.
    #[test]
    fn a_team_of_one_is_refused() {
        assert!(Team::new(0).is_none());
        assert!(Team::new(1).is_none());
    }

    /// Every slot of an SPMD region runs exactly once, and the barrier
    /// really separates phases: phase-two reads observe every phase-one
    /// write, across thousands of back-to-back regions.
    #[test]
    fn an_spmd_region_runs_every_slot_and_its_barrier_orders_phases() {
        let mut team = Team::new(4).expect("width 4");
        let width = team.width();
        for _ in 0..2_000 {
            let bar = Barrier::new(width);
            let ran: Vec<AtomicU32> = (0..width).map(|_| AtomicU32::new(0)).collect();
            let sums: Vec<AtomicU32> = (0..width).map(|_| AtomicU32::new(0)).collect();
            team.run_spmd(|slot| {
                let mut sense = false;
                ran[slot].fetch_add(1, Ordering::Relaxed);
                bar.wait(&mut sense);
                // Phase two: every slot sums every slot's phase-one mark.
                let total: u32 = ran.iter().map(|r| r.load(Ordering::Relaxed)).sum();
                sums[slot].store(total, Ordering::Relaxed);
                bar.wait(&mut sense);
            });
            assert!(ran.iter().all(|r| r.load(Ordering::Relaxed) == 1));
            assert!(sums
                .iter()
                .all(|s| s.load(Ordering::Relaxed) == width as u32));
        }
    }

    /// Chunked and SPMD jobs interleave on one team without confusing
    /// each other's dispatch.
    #[test]
    fn chunked_and_spmd_jobs_interleave() {
        let mut team = Team::new(3).expect("width 3");
        let width = team.width();
        let total = AtomicU32::new(0);
        for round in 0..1_000u32 {
            if round % 2 == 0 {
                team.run(50, |_| {
                    total.fetch_add(1, Ordering::Relaxed);
                });
            } else {
                team.run_spmd(|_| {
                    total.fetch_add(1, Ordering::Relaxed);
                });
            }
        }
        assert_eq!(total.load(Ordering::Relaxed), 500 * 50 + 500 * width as u32);
    }
}
