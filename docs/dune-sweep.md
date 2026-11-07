# Varreduras paramétricas e Monte Carlo (issue #10, D-0028 trilha 1)

O SPICE3 não tem `.step`: variar um valor é rodar o deck de novo à
mão. A varredura do Dune vive na **API**, não no deck — o deck fica
SPICE3 puro e sem cartão novo (o `.step` do PSpice não tem oráculo; o
ngspice em modo batch também não).

## Contrato

- O core recebe **conjuntos concretos** de atribuições: "rode estas N
  linhas de valores". Grade (produto cartesiano) e Monte Carlo
  (sorteios) são da linguagem hospedeira — **zero RNG no core**; o MC é
  reproduzível pela seed do chamador (`set.seed()`,
  `np.random.default_rng(seed)`).
- **Reconstrução por ponto (v1):** o deck é parseado uma vez; cada
  rodada copia a IR (`Netlist: Clone`), escreve os valores, elabora com
  `Circuit::from_netlist` e roda todos os cartões de análise por
  `Circuit::run` — o mesmo caminho do `dune -b`. Logo uma varredura
  reproduz N execuções individuais **bit a bit** (testado em Rust,
  Python e R contra o deck editado em texto).
- Sem estado entre rodadas (nem ponto de operação, nem fatoração). O
  *warm start* / `alter` é a issue #12; a costura está descrita abaixo.
- Uma rodada que falha (não converge — rotina na cauda de um MC) não
  derruba as outras no core (`SweepRun::outcome` é um `Result` por
  rodada); os bindings v1 levantam erro na primeira falha, nomeando a
  rodada e seus valores.

## O que varia (v1)

| texto | grandeza |
|---|---|
| `temp` | temperatura do circuito, °C (como `.options temp=`) |
| `r1`, `c1`, `l1` | valor do elemento |
| `vin`, `iin` | nível DC da fonte |
| `@r1[resistance]` / `@r1[r]` | valor do resistor |
| `@c1[capacitance]` / `@c1[c]` | valor do capacitor |
| `@l1[inductance]` / `@l1[l]` | valor do indutor |
| `@vin[dc]`, `@vin[acmag]`, `@vin[acphase]` | DC, módulo AC e fase AC (graus) de V/I |
| `@m1[w]`, `@m1[l]` | W e L desenhados do MOSFET (m) |

Sintaxe `@dev[param]` do ngspice, mais os atalhos de nome nu. Nomes
sem diferença de caixa; elementos de subcircuito pelo nome expandido
(`r.x1.r1`). Parâmetros de **modelo** ficam para a v2. Erros de
varredura (elemento inexistente, parâmetro que o elemento não tem, o
mesmo alvo duas vezes — inclusive `r1` e `@r1[r]` —, linha de
tamanho errado, valor não finito) são checados antes de qualquer
rodada.

Fonte sem especificação AC que recebe `acmag` (ou `acphase`) ganha
uma com a outra metade zero — módulo 0 excita nada, como sem spec.

## Superfície por linguagem

### Rust (`dune-core`)

```rust
pub enum SweepParam { Temp, Instance { name: String, param: Option<String> } }
impl SweepParam { fn parse(&str) -> Result<Self, Error>; fn key(&self) -> String }
pub struct SweepOutput { pub solutions: Vec<Solution>, pub notes: Vec<String> }
pub struct SweepRun { pub values: Vec<f64>, pub outcome: Result<SweepOutput, Error> }
pub fn sweep(nl: &Netlist, opts: &Options, params: &[SweepParam], runs: &[Vec<f64>])
    -> Result<Vec<SweepRun>, Error>
#[cfg(feature = "parallel")]
pub fn sweep_parallel(nl, opts, params, runs, threads: usize) -> Result<Vec<SweepRun>, Error>
```

`opts` já traz o `.options` do deck e os overrides do chamador; um
`temp` varrido substitui `opts.temp` na rodada.

### Python (`spice`)

```python
spice.sweep(netlist, params: Mapping[str, ArrayLike], *, mode="grid" | "zip",
            includes=None, options=None, threads=1) -> spice.SweepResult
```

`SweepResult` é uma sequência de `Result` (uma por rodada, índice
0-based) com `.names`, `.params` (polars: `run` + uma coluna por
parâmetro) e `.df(plot)` — o `Plot.df()` de cada rodada empilhado, com
`run` e os parâmetros na frente (`pandas=True` também). Grade na ordem
do `itertools.product` (primeiro parâmetro mais lento).

### R (`spice`)

```r
spice_sweep(deck, ..., .mode = c("grid", "zip"), .includes = NULL,
            .options = NULL, .threads = 1L)
```

`...` são vetores nomeados (`` `@m1[w]` = ... `` com crases); um data
frame de sorteios entra com `!!!draws` ou como único argumento.
Devolve `spice_sweep` (`title`, `params` = tibble `run` 1-based +
parâmetros, `runs` = lista de `spice_result`, `diagnostics`);
`spice_tidy(sweep, plots =)` dá a tibble longa de sempre com `run` e
os parâmetros na frente. Grade na ordem do `tidyr::expand_grid()`
(primeiro mais lento — a mesma do Python). `zip` recicla comprimento 1.

### wasm / JS

Fora desta leva: o `dune-core` compila para `wasm32` com `sweep`
sequencial (a feature `parallel` nunca entra no wasm), mas o schema
JSON do `dune-wasm` é superfície própria (pausa de naming) e fica para
quando a trilha JS pedir.

## Paralelismo

Feature nativa `parallel` do `dune-core` (rayon, opcional, off por
default; o wasm nunca a liga). Python e R a ligam; o default dos dois
é `threads = 1` (caminho sequencial — e o CRAN pede ≤ 2 núcleos nos
checks). `0` = um por núcleo.

Cada rodada é um circuito independente, sem redução entre rodadas, e
o backend faer roda sem paralelismo interno: o resultado paralelo é
**bit-idêntico** ao sequencial. O CI gateia isso
(`cargo test -p dune-core --features parallel --test sweep`, com 0, 1,
3 e 8 threads); pytest e testthat repetem a checagem pelos bindings.

Medição (2026-11-07, Apple Silicon 10 núcleos — 6P+4E —, wheel
`--release`, `spice.sweep(..., mode="zip")`, 128 rodadas, melhor de
3; máquina com load ~20 de outros jobs, então os números são
pessimistas e ruidosos):

| deck | 1 thread | 2 | 4 | 6 | 10 |
|---|---|---|---|---|---|
| `rca3040` (BJT, .tran; `temp` sorteada) | 610 ms | 302 ms (2,0×) | 170 ms (3,6×) | 139 ms (4,4×) | 96–122 ms (5–6,4×) |
| `mosmem` (12 MOS, .tran; `@m1[w]`) | 208 ms | 78 ms | 52 ms (4,0×) | 43 ms (4,9×) | 29 ms (7,1×) |
| `diffpair` (.tf+.dc+.ac+.tran; `rc1`) | 277 ms | 117 ms | 79 ms (3,5×) | 61 ms (4,5×) | 52 ms (5,3×) |

Todas as linhas bit-idênticas ao `threads=1`. Escala quase linear até
os núcleos P; os núcleos E rendem menos.

## Costura para a issue #12 (re-solve / alter)

- `SweepParam` é o endereçamento comum: o `alter` deve aceitar o mesmo
  texto (`@m1[w]`, `r1`, `temp`) e resolver para o mesmo alvo.
- Hoje o alvo é resolvido na **IR** (`Slot` privado em `sweep.rs`) e o
  circuito é reconstruído. Com o alter, `run_one` passa a: elaborar uma
  vez por *chunk*, aplicar as atribuições direto nos dispositivos e
  reaproveitar ponto de operação/fatoração. O teste bit a bit continua
  valendo para o caminho por reconstrução; o warm start seria opt-in
  (convergência a partir de outro chute pode mudar os últimos ulps).
