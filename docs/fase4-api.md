# Fase 4 — proposta P-2 (API pública) e P-6 (nomes)

Status: P-6 decidido em 2026-11-06 (D-0014: pacotes `spice`, prefixo
`spice_*`). P-2 em revisão pelo usuário — este arquivo é o inventário
completo; aprovado, vira D-0015 no
`DECISIONS.md` local. Este arquivo fica como referência versionada.

## P-6 — nomes dos pacotes

- **R:** pacote **`spice`** (CRAN livre em 2026-11-06; `SPICE` também,
  e o CRAN não aceita nomes que diferem só por caixa, então nada
  bloqueia).
- **Python:** distribuição **`spice`** no PyPI (livre em 2026-11-06 —
  verificado pela API JSON; `pyspice` é de terceiro, wrapper do
  ngspice). Módulo `import spice`. **Reservar cedo**: o PyPI é por
  ordem de chegada; publicar um 0.0.1 placeholder assim que o
  `dune-py` compilar.
- **Rust:** crates seguem `dune-*` (nome interno do motor; não se
  publica no crates.io por ora).
- **Prefixo das funções R:** decidido — **`spice_*`** (prefixo =
  pacote; o CLAUDE.md foi atualizado).

## P-2 — inventário completo da superfície pública do `dune-core`

Tudo que `dune_core::` exporta hoje, com as assinaturas. Campos de
modelo são os parâmetros do cartão 3f5 (nomes idênticos) e não são
listados um a um quando passam de ~25.

### Circuit

```rust
Circuit::new(title: impl Into<String>) -> Circuit
Circuit::add(&mut self, spec: impl Into<Spec>) -> Result<(), Error>
Circuit::nodeset(&mut self, node, value: f64)
Circuit::ic(&mut self, node, value: f64)
Circuit::from_netlist(&Netlist) -> Result<(Circuit, Vec<String>), Error>
Circuit::len(&self) -> usize            // + is_empty
// campo público: title: String
```

### Análises (todas `&self`, retornam `Result<T, Error>`)

```rust
op(opts)
dc(opts, sweeps: &[DcSweep])                       // 1 ou 2 varreduras
tran(opts, tstep, tstop, tstart, tmax: Option<f64>, uic: bool)
ac(opts, sweep: AcSweep, points, fstart, fstop)
pz(opts, nodes: [&str; 4], input: PzInput, which: PzWhich)
noise(opts, output: &str, output_ref: Option<&str>, src: &str,
      sweep, points, fstart, fstop, pts_per_summary: Option<usize>)
disto(opts, sweep, points, fstart, fstop, f2overf1: Option<f64>)
tf(opts, output: &OutputVar, insrc: &str)
sens(opts, output: &OutputVar, filters: &[String], ac: Option<SensAc>)
```

Ordem dos argumentos espelha os cartões (`.tran tstep tstop <tstart
<tmax>> <uic>` etc.).

### Resultados

| Tipo | Campos |
|---|---|
| `Op`, `Dc`, `Tran`, `Pz`, `Sens` | `solution: Solution`, `notes: Vec<String>` |
| `Ac` | idem |
| `Noise` | `density: Solution`, `integrated: Option<Solution>`, `notes` |
| `Disto` | `solutions: Vec<Solution>` (2 ou 3 plots), `notes` |
| `Tf` | `solution` + `transfer_function`, `input_impedance`, `output_impedance: f64`, `notes` |

`Solution { plotname: String, variables: Vec<(String, VarType)>,
data: Data, scale: Option<usize> }` com `num_points()`,
`column(name)`, `column_complex(name)`, `to_plot(title, date)`,
`select(&[OutExpr]) -> Selection`.

### Seleção

`OutExpr::{Voltage{node, reference, part}, Current{source, part},
Name, All}` + `OutExpr::parse("vm(3)")`; `Part::{Real, Imag, Mag,
Phase, Db}`; `Selection { scale, columns: Vec<OutColumn> }`;
`OutColumn { label, unit: VarType, data: ColData::{Real,Complex} }`.

### Specs de dispositivo (todos com `From<T> for Spec`)

| Spec | Campos além de `name`/`nodes` |
|---|---|
| `Resistor` [2 nós] | `value` |
| `Capacitor`/`Inductor` [2] | `value`, `ic: Option<f64>` |
| `Mutual` | `l1`, `l2`, `k` |
| `Vsource`/`Isource` [2] | `dc: Option<f64>`, `wave: Option<Wave>`, `ac: Option<(mag, fase°)>`, `distof1/2: Option<(mag, fase°)>` |
| `Vcvs`/`Vccs` [4] | `gain` |
| `Cccs`/`Ccvs` [2] | `ctrl` (nome da Vsource), `gain` |
| `Diode` [2] | `model`, `area`, `off`, `temp: Option<f64>`, `ic: Option<f64>` |
| `Bjt` [4: c b e subs] | `model`, `area`, `off`, `ic: (vce, vbe)`, `temp` |
| `Mos1/2/3/6`, `Bsim1/2` [4: d g s b] | `model`, `l/w/ad/as_: Option<f64>`, `pd/ps/nrd/nrs: f64`, `off`, `ic: (vds, vgs, vbs)`, `temp` (só MOS1–6) |
| `Jfet`/`Mes` [3: d g s] | `model`, `area`, `off`, `ic: (vds, vgs)`, `temp` (só JFET) |
| `Ltra` [4] | `model: LtraModel` |
| `Tra` [4] | `z0`, `td/f/nl: Option<f64>`, `ic: [V1, I1, V2, I2]` |
| `SwitchV` [4] / `SwitchI` [2] | `model`, `on` (+ `ctrl` no W) |

Modelos: `DiodeModel` (15 campos), `BjtModel` (42), `Mos1Model` (31),
`Mos2Model` (38), `Mos3Model` (37), `Mos6Model` (40), `Bsim1Model`
(78), `Bsim2Model` (134), `JfetModel` (15), `MesModel` (15),
`SwModel`/`CswModel` (4), `LtraModel` (13) — todos `Default` com os
defaults do 3f5 e `new(tipo)` onde há NMOS/PMOS etc.

### Suporte

`Options` (23 campos, defaults 3f5), `Wave::{Pulse, Sin, Exp, Pwl,
Sffm}(Vec<f64>)` (argumentos posicionais do cartão),
`Method::{Trapezoidal, Gear}`, `AcSweep::{Decade, Octave, Linear}`,
`DcSweep { src, start, stop, step }`, `SensAc`, `OutputVar`,
`PzInput`, `PzWhich`, `LtraInterp`, `Error` (7 variantes, com
`Unsupported{name, what}`, `Singular`, `Timestep{time}`…).

## Pontos levantados na revisão (para decisão)

1. **`noise` destoa**: recebe `output: &str, output_ref: Option<&str>`
   onde `tf`/`sens` usam `&OutputVar`. Proposta: unificar em
   `&OutputVar` antes de congelar (quebra pequena, só interna).
2. **Tipos do `dune-raw` expostos sem re-export**: `Solution.data:
   Data` e `variables: VarType` obrigam o usuário a importar
   `dune_raw`. Proposta: `pub use dune_raw::{Data, VarType};` no
   `dune-core`.
3. **`Wave` posicional** (`Pulse(Vec<f64>)`): fiel ao cartão; as
   bordas (R/Py) nomeiam os argumentos. Alternativa seria structs
   nomeados por forma de onda — proposta: manter posicional no core.
4. **Tuplas de `ic`** (`(vds, vgs, vbs)` etc.): idem — as bordas
   nomeiam. Manter.
5. **`as_`** (keyword Rust): sai como `as` nas bordas. Manter.
6. **BJT de 3 nós**: na construção programática o substrato é
   explícito (passe `"0"`); o parser preenche sozinho. Manter.

## Regras para as bordas (FFI/wasm/R/Py)

- Campos com nome Rust-contorcido (`as_` dos MOS) saem com o nome do
  cartão (`as`) nas interfaces externas.
- Erros atravessam como mensagem estruturada com span; pânico nunca
  cruza a borda.
- `Solution` é a unidade de troca (JSON no wasm; tibble longo no R;
  polars no Python — mesma informação, idioma de cada casa).
- Schema JSON do `dune-wasm`: espelho campo a campo do `Plot` do
  `dune-raw` (anexado quando o crate fechar).

## Correções feitas nesta revisão

- `Bjt` não estava re-exportado (único dispositivo não construível
  programaticamente) — consertado em 9d036b1.
