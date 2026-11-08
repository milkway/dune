//! The `Session` class (issue #12): a deck or a pic figure parsed,
//! extracted and elaborated once, then altered and re-run — the
//! engine behind the Studio's sliders.
//!
//! [`SimSession`] is the native (and tested) half; [`JsSession`] is
//! its `wasm_bindgen` face, exported to JS as `Session`. A run's
//! result has exactly the shape (and, with warm starts off, exactly
//! the content) `simulate` / `simulate_typed` give for the altered
//! deck: the elaboration notes and the parser/`.options` warnings of
//! the original deck come with every run.

use dune_core::{AlterValue, Pss, PssParams, Session, WarmStats};
use wasm_bindgen::prelude::*;

use crate::overrides::{Overrides, check_divisor};
use crate::schema::{PicJson, PicSimOptions, SimError, SimOptions};
use crate::{
    Card, Outcome, Prepared, RunData, extract_runnable, guarded, js_error, opts_text,
    parse_options, prepare, pss_params, to_js, typed_result,
};

/// A live circuit: [`dune_core::Session`] plus what a fresh run's
/// result carries besides the plots.
pub struct SimSession {
    session: Session,
    /// The deck's parse/elaboration context minus the circuit (moved
    /// into the session); `base.options` stays the deck's own.
    base: Prepared,
    pic: Option<PicJson>,
    /// `options` / `tmaxDivisor` of the session (issue #16).
    overrides: Overrides,
    /// The last [`SimSession::set_temp`], °C: it wins over the deck's
    /// and the overrides' `temp`.
    temp: Option<f64>,
}

impl SimSession {
    fn from_prepared(mut base: Prepared, pic: Option<PicJson>, overrides: Overrides) -> Self {
        let circuit = std::mem::take(&mut base.circuit);
        let session = Session::new(circuit, base.options.clone(), base.analyses.clone());
        let mut s = SimSession {
            session,
            base,
            pic,
            overrides,
            temp: None,
        };
        s.retune();
        s
    }

    /// The session's options from scratch: the deck's, the overrides
    /// on top, then the `set_temp` temperature.
    fn retune(&mut self) {
        let (mut options, diags) = self.overrides.apply(&self.base.options);
        if let Some(t) = self.temp {
            // `Session::set_temp` validated it; same `+ CTOK`.
            options.temp = t + dune_core::consts::CTOK;
        }
        *self.session.options_mut() = options;
        self.base.override_diagnostics = diags;
    }

    /// Parse and elaborate a deck (`opts` as in `simulate`).
    pub fn open(netlist: &str, opts: &SimOptions) -> Result<Self, SimError> {
        guarded(|| {
            let overrides = opts.overrides();
            overrides.check()?;
            Ok(SimSession::from_prepared(
                prepare(netlist, &opts.includes)?,
                None,
                overrides,
            ))
        })
    }

    /// Extract a figure's deck, then parse and elaborate it (errors as
    /// `simulate_pic`'s, the extraction riding on them).
    pub fn open_pic(src: &str, opts: &PicSimOptions) -> Result<Self, SimError> {
        guarded(|| {
            let overrides = opts.overrides();
            overrides.check()?;
            let pic = extract_runnable(src, opts.snap)?;
            match prepare(&pic.netlist, &opts.includes) {
                Ok(p) => Ok(SimSession::from_prepared(p, Some(pic), overrides)),
                Err(mut e) => {
                    e.pic = Some(Box::new(pic));
                    Err(e)
                }
            }
        })
    }

    /// The extraction of a session opened from a figure.
    pub fn pic(&self) -> Option<&PicJson> {
        self.pic.as_ref()
    }

    /// [`dune_core::Circuit::alter`]; a refused alteration is kind
    /// `alter` and leaves the circuit as it was.
    pub fn alter(&mut self, refdes: &str, param: &str, value: AlterValue) -> Result<(), SimError> {
        self.session
            .alter(refdes, param, value)
            .map_err(|e| SimError::new("alter", e.to_string()))
    }

    /// [`dune_core::Circuit::param`] (kind `alter` on an unknown
    /// element or parameter).
    pub fn param(&self, refdes: &str, param: &str) -> Result<Option<AlterValue>, SimError> {
        self.session
            .param(refdes, param)
            .map_err(|e| SimError::new("alter", e.to_string()))
    }

    /// The parameters `alter` accepts for `refdes`.
    pub fn alterable(&self, refdes: &str) -> Result<&'static [&'static str], SimError> {
        self.session
            .circuit()
            .alterable(refdes)
            .map_err(|e| SimError::new("alter", e.to_string()))
    }

    /// Circuit temperature, °C. Wins over the deck's and the
    /// overrides' `temp` from now on.
    pub fn set_temp(&mut self, celsius: f64) -> Result<(), SimError> {
        self.session
            .set_temp(celsius)
            .map_err(|e| SimError::new("alter", e.to_string()))?;
        self.temp = Some(celsius);
        Ok(())
    }

    /// Replace the `SimOptions.options` overrides (an empty map
    /// restores the deck's options) for the next runs; the circuit is
    /// kept. Invalid entries become warnings in each run's
    /// `diagnostics`, as when opening.
    pub fn set_options(&mut self, options: serde_json::Map<String, serde_json::Value>) {
        self.overrides.options = options;
        self.retune();
    }

    /// The current `options` overrides.
    pub fn options(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.overrides.options
    }

    /// Replace `tmaxDivisor` (`None`: the cards' TMAX as is); out of
    /// range is kind `options` and changes nothing.
    pub fn set_tmax_divisor(&mut self, d: Option<f64>) -> Result<(), SimError> {
        check_divisor(d)?;
        self.overrides.tmax_divisor = d;
        self.retune();
        Ok(())
    }

    pub fn tmax_divisor(&self) -> Option<f64> {
        self.overrides.tmax_divisor
    }

    pub fn temp(&self) -> f64 {
        self.session.temp()
    }

    pub fn set_warm_start(&mut self, on: bool) {
        self.session.set_warm_start(on);
    }

    pub fn warm_start(&self) -> bool {
        self.session.warm_start()
    }

    pub fn warm_stats(&self) -> WarmStats {
        self.session.warm_stats()
    }

    /// Run every analysis card again; the result `simulate` gives for
    /// the altered deck.
    pub fn run(&mut self) -> Result<RunData, SimError> {
        guarded(|| {
            let mut out = Outcome::new(self.base.notes.clone());
            for analysis in self.session.analyses().to_vec() {
                let card = match pss_params(&analysis) {
                    Some(p) => self.run_pss(&p).map(|p| Card::Pss(Box::new(p))),
                    None => self.session.run_analysis(&analysis).map(Card::Plain),
                };
                out.push(card.map_err(SimError::analysis)?);
            }
            Ok(self.base.finish(out))
        })
    }

    /// A `.pss` card on the session's circuit, kept whole (period and
    /// history) instead of [`Session::run_analysis`]'s plots. Like
    /// every card of [`Session::run`], it re-runs cold when it failed
    /// after a warm-start miss; the cold run here turns the warm start
    /// off and on again, which also forgets the stored point (the next
    /// run starts cold, as after `set_warm_start(true)`).
    fn run_pss(&mut self, p: &PssParams) -> Result<Pss, dune_core::Error> {
        let misses = self.session.warm_stats().misses;
        let first = self.session.circuit().pss(self.session.options(), p);
        if first.is_ok() || self.session.warm_stats().misses == misses {
            return first;
        }
        self.session.set_warm_start(false);
        let cold = self.session.circuit().pss(self.session.options(), p);
        self.session.set_warm_start(true);
        cold
    }
}

/// JS `Session`: see the crate docs and `npm/index.d.ts`.
#[wasm_bindgen(js_name = Session)]
pub struct JsSession(SimSession);

/// JS value → [`AlterValue`]: a number, or an `Array` of numbers / a
/// `Float64Array` (a source function's arguments).
fn alter_value(v: &JsValue) -> Result<AlterValue, JsValue> {
    if let Some(x) = v.as_f64() {
        return Ok(AlterValue::Num(x));
    }
    if v.is_instance_of::<js_sys::Float64Array>() {
        let a: js_sys::Float64Array = v.clone().unchecked_into();
        return Ok(AlterValue::List(a.to_vec()));
    }
    if js_sys::Array::is_array(v) {
        let a: js_sys::Array = v.clone().unchecked_into();
        let mut out = Vec::with_capacity(a.length() as usize);
        for x in a.iter() {
            out.push(x.as_f64().ok_or_else(|| {
                js_error(&SimError::new(
                    "alter",
                    "a source function's arguments must all be numbers",
                ))
            })?);
        }
        return Ok(AlterValue::List(out));
    }
    Err(js_error(&SimError::new(
        "alter",
        "value must be a number, an Array of numbers or a Float64Array",
    )))
}

#[wasm_bindgen(js_class = Session)]
impl JsSession {
    /// `Session.fromDeck(netlist, opts?)` — `opts` as `simulate`'s.
    #[wasm_bindgen(js_name = fromDeck)]
    pub fn from_deck(netlist: &str, opts: JsValue) -> Result<JsSession, JsValue> {
        let opts_json = opts_text(&opts)?;
        let o: SimOptions = parse_options(opts_json.as_deref()).map_err(|e| js_error(&e))?;
        SimSession::open(netlist, &o)
            .map(JsSession)
            .map_err(|e| js_error(&e))
    }

    /// `Session.fromPic(src, opts?)` — `opts` as `simulate_pic`'s.
    #[wasm_bindgen(js_name = fromPic)]
    pub fn from_pic(src: &str, opts: JsValue) -> Result<JsSession, JsValue> {
        let opts_json = opts_text(&opts)?;
        let o: PicSimOptions = parse_options(opts_json.as_deref()).map_err(|e| js_error(&e))?;
        SimSession::open_pic(src, &o)
            .map(JsSession)
            .map_err(|e| js_error(&e))
    }

    /// `alter(refdes, param, value)`; `param` `""` is the element's
    /// principal value.
    pub fn alter(&mut self, refdes: &str, param: &str, value: JsValue) -> Result<(), JsValue> {
        let v = alter_value(&value)?;
        self.0.alter(refdes, param, v).map_err(|e| js_error(&e))
    }

    /// `param(refdes, param)` — a number, a `Float64Array` (a source
    /// function's arguments) or `null` (never given).
    pub fn param(&self, refdes: &str, param: &str) -> Result<JsValue, JsValue> {
        Ok(
            match self.0.param(refdes, param).map_err(|e| js_error(&e))? {
                Some(AlterValue::Num(x)) => JsValue::from_f64(x),
                Some(AlterValue::List(v)) => js_sys::Float64Array::from(v.as_slice()).into(),
                None => JsValue::NULL,
            },
        )
    }

    /// `alterable(refdes)` — the parameter names `alter` accepts.
    pub fn alterable(&self, refdes: &str) -> Result<js_sys::Array, JsValue> {
        let names = self.0.alterable(refdes).map_err(|e| js_error(&e))?;
        Ok(names.iter().map(|n| JsValue::from_str(n)).collect())
    }

    #[wasm_bindgen(js_name = setTemp)]
    pub fn set_temp(&mut self, celsius: f64) -> Result<(), JsValue> {
        self.0.set_temp(celsius).map_err(|e| js_error(&e))
    }

    pub fn temp(&self) -> f64 {
        self.0.temp()
    }

    /// `setOptions(options?)` — replace the `options` overrides
    /// (`SimOptions.options`; `null`/`undefined`/`{}` restore the
    /// deck's). Throws kind `options` when `options` is not an object.
    #[wasm_bindgen(js_name = setOptions)]
    pub fn set_options(&mut self, options: JsValue) -> Result<(), JsValue> {
        let map = match opts_text(&options)? {
            None => serde_json::Map::new(),
            Some(text) => match serde_json::from_str(&text) {
                Ok(serde_json::Value::Object(m)) => m,
                _ => {
                    return Err(js_error(&SimError::new(
                        "options",
                        "bad options: options must be an object",
                    )));
                }
            },
        };
        self.0.set_options(map);
        Ok(())
    }

    /// `options()` — the current `options` overrides, a plain object.
    pub fn options(&self) -> JsValue {
        to_js(self.0.options()).unwrap_or_else(|| js_sys::Object::new().into())
    }

    /// `setTmaxDivisor(d?)` — replace `tmaxDivisor` (`undefined`/`null`
    /// clears it). Throws kind `options` unless `d` is a finite number
    /// ≥ 1.
    #[wasm_bindgen(js_name = setTmaxDivisor)]
    pub fn set_tmax_divisor(&mut self, d: Option<f64>) -> Result<(), JsValue> {
        self.0.set_tmax_divisor(d).map_err(|e| js_error(&e))
    }

    /// `tmaxDivisor()` — the divisor, or `undefined`.
    #[wasm_bindgen(js_name = tmaxDivisor)]
    pub fn tmax_divisor(&self) -> Option<f64> {
        self.0.tmax_divisor()
    }

    #[wasm_bindgen(js_name = setWarmStart)]
    pub fn set_warm_start(&mut self, on: bool) {
        self.0.set_warm_start(on);
    }

    #[wasm_bindgen(js_name = warmStart)]
    pub fn warm_start(&self) -> bool {
        self.0.warm_start()
    }

    /// `{ hits, misses }`.
    #[wasm_bindgen(js_name = warmStats)]
    pub fn warm_stats(&self) -> JsValue {
        let s = self.0.warm_stats();
        let o = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&o, &"hits".into(), &(s.hits as f64).into());
        let _ = js_sys::Reflect::set(&o, &"misses".into(), &(s.misses as f64).into());
        o.into()
    }

    /// The extraction (`PicJson`) of a session from a figure, else
    /// `undefined`.
    pub fn pic(&self) -> JsValue {
        self.0.pic().and_then(to_js).unwrap_or(JsValue::UNDEFINED)
    }

    /// `run()` — `simulate`'s result for the current circuit.
    pub fn run(&mut self) -> Result<JsValue, JsValue> {
        let data = self.0.run().map_err(|e| js_error(&e))?;
        let out = serde_json::to_string(&data.to_json())
            .map_err(|e| js_error(&SimError::new("internal", e.to_string())))?;
        js_sys::JSON::parse(&out)
    }

    /// `runTyped()` — `simulate_typed`'s result for the current
    /// circuit.
    #[wasm_bindgen(js_name = runTyped)]
    pub fn run_typed(&mut self) -> Result<JsValue, JsValue> {
        let data = self.0.run().map_err(|e| js_error(&e))?;
        let header = serde_json::to_string(&data.header_json())
            .map_err(|e| js_error(&SimError::new("internal", e.to_string())))?;
        typed_result(&header, &data.plots)
    }
}
