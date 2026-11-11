# Fase 4 — proposta P-2 (API pública) e P-6 (nomes)

Status: P-6 decidido em 2026-11-06 (D-0014: pacotes `spice`, prefixo
`spice_*`). P-2 em revisão pelo usuário — este arquivo é o inventário
completo; aprovado, vira D-0015 no
`DECISIONS.md` local. Este arquivo fica como referência versionada.

## P-6 — nomes dos pacotes

- **R:** pacote **`spice`** (CRAN livre em 2026-11-06; `SPICE` também,
  e o CRAN não aceita nomes que diferem só por caixa, então nada
  bloqueia).
- **Python:** distribuição **`spice3`** no PyPI (D-0017: `spice`
  está registrado sem releases — o `simple/` index responde 200 e a
  API JSON engana com 404; o PyPI não libera o nome). Módulo
  **`import spice`** (dist ≠ módulo, padrão sklearn). PEP 541 pelo
  nome `spice` corre em paralelo pela conta do usuário.
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
sp(opts, sweep, points, fstart, fstop, donoise: bool)   # D-0018
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
| `Vsource`/`Isource` [2] | `dc: Option<f64>`, `wave: Option<Wave>`, `ac: Option<(mag, fase°)>`, `distof1/2: Option<(mag, fase°)>`; Vsource ainda `port: Option<RfPort{num, z0}>` (D-0018 — porta RF muda a fonte em TODAS as análises, como no ngspice) |
| `Vcvs`/`Vccs` [4] | `gain` |
| `Cccs`/`Ccvs` [2] | `ctrl` (nome da Vsource), `gain` |
| `Diode` [2] | `model`, `area`, `off`, `temp: Option<f64>`, `ic: Option<f64>` |
| `Bjt` [4: c b e subs] | `model`, `area`, `off`, `ic: (vce, vbe)`, `temp` |
| `Mos1/2/3/6`, `Bsim1/2` [4: d g s b] | `model`, `l/w/ad/as_: Option<f64>`, `pd/ps/nrd/nrs: f64`, `off`, `ic: (vds, vgs, vbs)`, `temp` (só MOS1–6) |
| `Jfet`/`Mes` [3: d g s] | `model`, `area`, `off`, `ic: (vds, vgs)`, `temp` (só JFET) |
| `Ltra` [4] | `model: LtraModel` |
| `Tra` [4] | `z0`, `td/f/nl: Option<f64>`, `ic: [V1, I1, V2, I2]` |
| `SwitchV` [4] / `SwitchI` [2] | `model`, `on` (+ `ctrl` no W) |
| `Asrc` [2] *(proposto em 2026-11-08, fora da v0 congelada — pede decisão)* | `kind: SourceKind` (V=/I=), `expr: dune_netlist::ir::Expr` (de `dune_netlist::parse_expr`) |

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

0. **`Circuit::run(&ir::Analysis, &Options)`**: o despacho
   cartão-de-análise → chamada está triplicado (dune-cli,
   dune-oracle, dune-wasm) e R/Python fariam a 4ª e a 5ª cópias.
   Proposta: um método único no core devolvendo
   `(Vec<Solution>, Vec<String>)`; os quatro consumidores o usam.

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
7. **Temperatura de instância fora dos specs** *(proposta futura,
   2026-11-08 — pede decisão)*: o `DTEMP=` do ngspice-47 (R, diodo,
   BJT, JFET, MOS1/2/3/6) e o modelo/`TEMP=` do R entram hoje só
   pelo front-end do netlist, por tabelas crate-privadas do
   `Circuit` (`res_temp: ResTemp {tc1, tc2, tnom, temp, dtemp}` e
   `dtemp: nome → K`, padrão do `ic_given`); os specs públicos não
   mudaram. Quem constrói o circuito por spec tipado não alcança
   `DTEMP`, nem TC/`TEMP=` do R. Extensão proposta, quando o
   usuário pedir: `dtemp: Option<f64>` nos specs que já têm `temp`
   e `Resistor { model: Option<ResModel>, temp, dtemp }` (com
   `ResModel {rsh, defw, narrow, tc1, tc2, tnom}`), espelhado nos
   bindings no mesmo PR.

## Regras para as bordas (FFI/wasm/R/Py)

- Campos com nome Rust-contorcido (`as_` dos MOS) saem com o nome do
  cartão (`as`) nas interfaces externas.
- Erros atravessam como mensagem estruturada com span; pânico nunca
  cruza a borda.
- `Solution` é a unidade de troca (JSON no wasm; tibble longo no R;
  polars no Python — mesma informação, idioma de cada casa).
- Schema JSON do `dune-wasm` (fechado 2026-11-06): a única exportação
  JS é `simulate(netlist: string, opts?) -> object`; `opts` aceita só
  `{ includes: { "<path>": "<texto>" } }` (navegador não tem FS).
  Retorno `RawfileJson = { plots: [PlotJson], notes: [string],
  diagnostics: [DiagnosticJson] }`; `PlotJson` espelha o `Plot` do
  `dune-raw` campo a campo (`title, date, plotname, flags, commands,
  options, dimensions, variables[{name,type}], npoints, columns`) com
  complexos como pares `[re, im]` e NaN/±inf como `null`. Erros são
  `Error` JS com `.kind` (`options|parse|elaborate|analysis`) e
  `.diagnostics` (spans). Quem embute (Studio) deve chamar de um Web
  Worker terminável (loop/OOM em wasm não se interrompe de dentro).
  Medidas: 1.48 MB (`wasm-opt -Oz`: 1.23 MB; gzip: 425 KB); ~2×
  o tempo nativo (rca3040 full 13 ms vs 7 ms); LTRA 3–7× (libm de
  software).

## Correções feitas nesta revisão

- `Bjt` não estava re-exportado (único dispositivo não construível
  programaticamente) — consertado em 9d036b1.

## Pacotes entregues (2026-11-06)

### Python (`python/`, commit 46e7abe)

`spice.simulate(netlist, *, includes=None, options=None) -> Result`;
`Result` (plots/notes/diagnostics, sequência), `Plot`
(`variables`, `scale`, `column(name)` → numpy zero-cópia read-only,
`df()` → polars **wide** com complexos em `re:`/`im:`,
`df(pandas=True)`), `SpiceError` com `kind`
parse|elaborate|analysis|internal. Convenção: **`str` é texto do
deck, nunca caminho** (um `str` de uma linha que nomeia arquivo
existente é rejeitado com dica de `pathlib.Path`).

### R (`r/`, commit e694ae0)

`spice_run(deck, includes, options)` → `spice_result`;
`spice_plots()`, `spice_tidy()` → tibble **longo** (`plot, point,
scale_name, scale, variable, type, value | re/im`),
`spice_read_raw`/`spice_write_raw`, `print.*`, `as_tibble`,
`autoplot`. Erros `cli_abort` com classe `spice_error_<kind>`.
Convenção: **string única sem newline é caminho**; vetor de
caracteres ou texto com newlines é o deck.

### C (`crates/dune-ffi`, 2026-11-06)

`dune_simulate(text)`, `dune_simulate_with_includes(text, names,
texts, n)`, `dune_simulate_file(path)` → `DuneResult*` opaco (nunca
NULL); `dune_result_free` é a única desalocação. Acessores
`dune_result_{ok, error_kind, error_message, note_count, note,
diagnostic_*, plot_count}` e `dune_plot_{name, title,
variable_count, variable_name, variable_type, npoints, is_complex,
scale_index, column_real, column_complex}`. Colunas zero-cópia
emprestadas do resultado; complexo = pares `re,im` intercalados
(`out_len` em doubles = 2×npoints; layout de `double _Complex`).
`DuneErrorKind` NONE|INPUT|IO|PARSE|ELABORATE|ANALYSIS|INTERNAL.
Tolerante a NULL e índice inválido; pânico → INTERNAL via
catch_unwind. Header `include/dune.h` por cbindgen 0.29.4,
versionado (teste compara; `DUNE_FFI_BLESS=1` regenera); compila
como C99 e C++11 `-pedantic -Werror`. Smoke test em C no
`cargo test` (unix, com `cc`).

### Notebooks (2026-11-06)

- **knitr (R):** engine **`spice`** (supersede o nome `dune` do
  PLAN §2.6, por D-0014) registrado no `.onLoad` só se o knitr
  carregar; chunk ```{spice} = netlist; opções `spice.output`
  ("tidy" default | "raw" — `spice.output` e não `output`, que o
  Quarto já usa), `spice.var` (default = label do chunk; FALSE
  desliga), `spice.options`/`spice.includes`; erro com span para o
  knit (ou imprime com `error=TRUE`). `?spice_knitr`.
- **Jupyter (Python):** `Plot._repr_svg_()` — preview SVG
  640×360 autocontido, fundo transparente legível em claro/escuro,
  ticks com prefixos SI, x log em frequência >1 década, paleta Tol
  6 cores (7–8 tracejados, teto 8 + "(+N more)"), um eixo y (tipos
  diferentes anotados na legenda, não desenhados), decimação
  min/max ~400 pts, complexo = |valor|; sem escala → tabela HTML.
  `Result._repr_html_()` = tabela compacta + notes/diagnostics.
  Revisão visual feita (previews em .claude/scratch/previews/).

### Divergências deliberadas entre os espelhos (nota exigida pelo
CLAUDE.md §Pacotes)

- **Deck como string**: Python = sempre texto (há `Path` no idioma);
  R = uma linha sem newline é caminho (R não tem tipo de caminho e
  `spice_run("rc.cir")` é o uso natural). Mesma semântica de
  `includes` nos dois.
- **Forma tabular**: polars wide no Python (idioma de colunas),
  tibble longo no R (idioma ggplot/tidyverse) — mesma informação.
- **Kinds de erro**: R acrescenta `input`/`io`/`raw` (coisas que no
  Python viram `TypeError`/`FileNotFoundError` nativos); a C ABI usa
  os kinds do Python + `input`/`io` (sem exceções nativas) e separa
  texto/arquivo em funções distintas, então a ambiguidade
  string-caminho de R/Python não existe nela.

### Pendências da Fase 4

- CI: o smoke test C do dune-ffi precisa de `cc` no runner
  (ubuntu-latest tem; Windows pula).
- ~~Vendoring CRAN (crates por path + registry) e Makevars.win real.~~
  Feito na 0.1.0: `tools/r-vendor.sh` + build offline; sdist do
  Python instalável. Receita em `docs/release-py-r.md`.
- Placeholder `spice` no PyPI (wheel 0.0.1 pronto; publicação é do
  usuário).
- LICENSE na raiz do repo — confirmar titulares (proposta do pacote
  R: "André Leite and The Regents of the University of California",
  BSD-3, pela origem 3f5).
- Interrupção de simulação longa a partir de R/Python (sem check de
  interrupt no engine) — candidata a hook no core.
- Bug upstream do savvy 0.11 (`try_from_iter` vazio) contornado em
  `r/src/rust/src/lib.rs`; reportar.
