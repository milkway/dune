//! The transient operating point (`OPtran`) has a step budget
//! (D-0077). ngspice-47 marches that pseudo-transient without any
//! bound: on this deck — a VBIC pnp with excess phase (`TD`), whose
//! ngspice equations are wrong for the pnp (docs/vbic.md, Fidelidade)
//! — it needs 2.5e5, then 7.8e5, then more than 2e6 picosecond steps
//! per `.dc` point, and neither program finished in a day. A deck
//! must end, with a result or with an error.

use std::time::{Duration, Instant};

use dune_core::{Circuit, Options};

/// The probe that ran for 24 hours: `p24.cir` of the VBIC port.
const DECK: &str = "vbic pnp with excess phase
vb b 0 dc 0.6
vc c 0 dc 2
vs s 0 dc 0
q1 c b 0 s nv
ebp bp 0 b 0 -1
vcp cp 0 dc -2
vsp sp 0 dc 0
q2 cp bp 0 sp pv
.model nv npn (level=4 is=1e-16 nf=1.0 nr=1.0 ibei=1e-18 nei=1 iben=1e-15 nen=2 ibci=2e-17 nci=1.01
+ ibcn=5e-15 ncn=2 vef=50 ver=5 ikf=5e-3 ikr=5e-4 nkf=0.5
+ rcx=10 rci=50 vo=1.5 gamm=2e-11 hrcf=1 rbx=20 rbi=80 re=2 rs=40 rbp=100
+ isp=1e-17 wsp=1 nfp=1 ibeip=1e-19 ibenp=1e-16 ibcip=1e-18 ncip=1 ibcnp=1e-15 ncnp=2 ikp=1e-4
+ cje=50f pe=0.8 me=0.35 aje=-0.5 cjc=30f pc=0.7 mc=0.3 ajc=-0.5 cjep=10f cjcp=80f ps=0.6 ms=0.3
+ cbeo=5f cbco=3f qco=1e-15 ccso=2f tf=10p qtf=0.1 xtf=5 vtf=1 itf=10e-3 tr=1n
+ avc1=2 avc2=15 wbe=0.9)
.model pv pnp (level=9 is=5e-17 ibei=5e-19 iben=2e-15 ibci=1e-17 ibcn=1e-15 vef=30 ver=4 ikf=2e-3 td=10p
+ rcx=15 rci=80 vo=1 gamm=1e-11 rbx=30 rbi=100 re=3 qbm=1 isp=1e-17 ibcip=1e-18 ikp=1e-4)
.dc vb 0.3 1.1 0.01
.end
";

#[test]
fn a_crawling_transient_operating_point_ends_with_an_error() {
    let parsed = dune_netlist::parse_str("optran.cir", DECK);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut o = Options::default();
    o.apply(&parsed.netlist.options, &mut Vec::new());
    // A small budget keeps the test fast in a debug build; the
    // default (10 000 attempts) ends the same way, later.
    o.max_tran_steps = Some(400);
    let t0 = Instant::now();
    let r = c.run(&parsed.netlist.analyses[0], &o);
    let took = t0.elapsed();
    assert!(r.is_err(), "the sweep must fail, not crawl to a result");
    assert!(took < Duration::from_secs(60), "took {took:?}");
}
