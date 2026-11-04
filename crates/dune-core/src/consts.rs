//! Physical constants and reference values, byte for byte the 3f5's
//! (`corpus/spice3f5/src/include/const.h` and `lib/fte/ivars.c`).
//! Names in comments are the C names.

/// Electron charge, C (`CHARGE`).
pub const CHARGE: f64 = 1.6021918e-19;

/// 0 °C in kelvin (`CONSTCtoK`).
pub const CTOK: f64 = 273.15;

/// Boltzmann constant, J/K (`CONSTboltz`).
pub const BOLTZ: f64 = 1.3806226e-23;

/// Reference temperature, 27 °C in kelvin (`REFTEMP`).
pub const REFTEMP: f64 = 300.15;

/// k/q, V/K (`CONSTKoverQ`).
pub const KOVERQ: f64 = BOLTZ / CHARGE;

/// √2 (`CONSTroot2`).
pub const ROOT2: f64 = std::f64::consts::SQRT_2;

/// e (`CONSTe`).
pub const E: f64 = std::f64::consts::E;

/// Thermal voltage at `REFTEMP`, V (`CONSTvt0`).
pub const VT0: f64 = KOVERQ * REFTEMP;
