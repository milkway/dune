# Re-solve interativo — `Session` (issue #12, D-0028 trilha 3)

Proposta de API (pausa de naming) e notas de implementação. Nada aqui
está congelado: os nomes são a sugestão do agente para a revisão.

## Contrato

- **Bit a bit, por padrão.** Com o warm start desligado (default), um
  `alter` + `run` devolve exatamente o que a execução fresca do deck
  alterado devolve — mesmos plots, mesmos bits, mesmas notas e mesmos
  avisos (nos bindings). Isso vale por construção: o `alter` escreve no
  spec o mesmo campo, com a mesma operação, que a elaboração escreveria
  a partir do cartão alterado (`TEMP=` °C → `+ CTOK`, como o
  `elaborate.rs`; `.options temp` → `+ CTOK`, como `Options::apply`),
  e cada análise constrói o engine a partir dos specs como uma execução
  fresca.
- **"O deck alterado" quer dizer o cartão com o mesmo double.** O
  parser de números é o `INPevaluate` do 3f5, byte a byte: mantissa
  inteira × potência de dez — duas roundings. `10u` vira `10·1e-6 =
  9.999999999999999e-06`, não `1e-05`. Os testes alteram com o double
  que o parser produz para o texto (`val()` no Rust, `card()` em
  Python/R); um `alter(1e-5)` equivale ao cartão `1e-5`, não ao `10u`.
- **Warm start é opt-in e troca bits por custo.** Ligado, o ponto de
  operação *avulso* (`MODEDCOP`: `.op` e o bias de `.ac`, `.noise`,
  `.tf`, `.sens`, `.pz`, `.disto`, `.sp`) tenta primeiro um `NIiter` em
  `INITFLOAT` a partir da solução e do banco de estados 0 do ponto
  anterior — a mesma continuação que o `DCtrCurv` usa entre pontos de
  sweep. Convergido, é solução às mesmas tolerâncias
  (`RELTOL`/`VNTOL`/`ABSTOL`), não os mesmos bits. Se não convergir, a
  análise inteira é refeita a frio com engine novo — o resultado de um
  miss é o da execução fresca, bit a bit (gate
  `a_warm_miss_reruns_cold_and_gives_the_fresh_bits`).
  Nunca warm: decks com `.nodeset`, o ponto de operação do transiente
  (`TRANOP` — o grid de tempo do LTE depende dele; mudaria a forma de
  onda, não só o custo) e os pontos do `.dc`. Circuitos multiestáveis
  (latch, schmitt) podem assentar em outro ponto de equilíbrio, como
  um sweep faria — documentado, não gateado.

## O que é reutilizado e o que é refeito

| etapa | execução fresca | `Session::run` |
|---|---|---|
| leitura + `.include` | sim | **não** |
| parse (lexer, cartões, `.subckt`/expansão, `.param`) | sim | **não** |
| elaboração (`.model`, parâmetros de instância → specs) | sim | **não** |
| extração pic (rpic compile + extrator) — bindings | sim | **não** |
| `build` (setup + temperatura dos dispositivos) | por análise | por análise |
| `Engine::new` (alocação de slots, padrão, `analyze`) | por análise | por análise |
| fatoração numérica + solves | sim | sim |
| ponto de operação | frio | frio (default) / warm (opt-in) |

Por que `build`/`Engine::new` não são reaproveitados: medido, são
~15–20 % de um `.op` pequeno (bsim3_inv: build 8 µs + engine 5.5 µs de
32 µs), e reaproveitá-los exige resetar o estado mutável interno de 25
tipos de dispositivo para garantir o bit a bit (ou `Clone` em todos os
`Device`). Fica como passo futuro com gate próprio.

**Padrão simbólico (D-0005):** o backend default é o `faer` **denso**
(D-0013). O `analyze` dele só dimensiona as matrizes — não há fatoração
simbólica para guardar; o padrão MNA não muda com um `alter` (a
topologia é fixa: nenhum parâmetro alterável cria nó interno). Quando o
backend esparso entrar, o lugar natural do cache simbólico é o mesmo
canal da sessão (`WarmSlot`, abaixo), chaveado pela igualdade do
`Pattern`.

## API proposta

### Rust (`dune-core`)

```rust
// Circuit (programático ou de deck)
circuit.alter("r1", "", 2.2e3)?;                       // valor principal
circuit.alter("m1", "w", 4e-6)?;                       // nome ngspice
circuit.alter("vin", "pulse", vec![0.0, 1.0, 1e-9])?;  // função transiente
circuit.param("m1", "w")? -> Option<AlterValue>;       // p/ semear slider
circuit.alterable("m1")? -> &'static [&'static str];

pub enum AlterValue { Num(f64), List(Vec<f64>) }       // From<f64>, From<Vec<f64>>, From<&[f64]>

// Session = Circuit + Options + cartões de análise, vivos
let (mut s, notes) = Session::from_netlist(&parsed.netlist)?;
let s = Session::new(circuit, options, analyses);
s.alter(..)?; s.param(..)?;
s.set_temp(85.0)?; s.temp();            // °C, como `.options temp`
s.set_warm_start(true); s.warm_start(); s.warm_stats() -> WarmStats { hits, misses }
s.run()? -> (Vec<Solution>, Vec<String>)            // todos os cartões
s.run_analysis(&card)? -> (Vec<Solution>, Vec<String>)  // um cartão qualquer
s.circuit(); s.options(); s.options_mut(); s.analyses();
s.clone()   // sessão independente (mesmo warm-start setting, sem histórico)
```

Parâmetros (`alter`, nomes ngspice; `""` = principal):

| elemento | `""` → | demais |
|---|---|---|
| R | `resistance` (`r`) | — |
| C / L | `capacitance` (`c`) / `inductance` (`l`) | `ic` |
| V / I | `dc` | `acmag`, `acphase` (graus), `pulse` `sin` `exp` `pwl` `sffm` (lista) |
| E F G H | `gain` | — |
| MOS 1/2/3/6 | — | `w`, `l`, `temp` (°C) |
| BSIM1/2/3 | — | `w`, `l` |
| D, Q, J | `area` | `temp` (°C) |
| Z | `area` | — |

Erros: `Error::Build` (elemento/parâmetro desconhecido — a mensagem
lista os válidos —, não finito, R = 0, W/L/área ≤ 0, número onde cabe
lista e vice-versa). Uma alteração recusada não muda nada.

### JS (`dune-wasm` / npm `@strategicprojects/dune-sim`)

```js
const s = Session.fromDeck(netlist, { includes });
const s = Session.fromPic(src, { snap, includes });   // s.pic = extração (mapa de overlay)
s.alter('R1', 2200).alter('M1', 'w', 4e-6).alter('V1', 'pulse', [0, 1, 1e-9]);
s.param('R1'); s.param('V1', 'pulse');   // number | Float64Array | null
s.alterable('M1');                        // ['w', 'l', 'temp']
s.temp = 85; s.warmStart = true; s.warmStats;   // {hits, misses}
s.options = { trtol: 1 }; s.tmaxDivisor = 10;  // "passos finos" (issue #16)
s.options = null; s.tmaxDivisor = null;        // volta ao deck
s.run();        // === simulate(deck alterado)
s.runTyped();   // === simulateTyped(deck alterado)
s.free();       // ou `using s = Session.fromPic(...)`
```

Erros: `SimError` com `kind` novo `'alter'`. A classe JS é um wrapper
fino (`index.js`) sobre a classe `Session` do wasm-bindgen.

`options`/`tmaxDivisor` (issue #16; também em `SimOptions` de todo
`simulate*`/`from*`): `options` são chaves de `.options` aplicadas
depois dos cartões do deck pelo próprio `Options::apply` (entrada
inválida = aviso em `diagnostics`, prefixo `opts.options:`, nunca
exceção; número entra exato, string é lida como o texto do cartão);
`tmaxDivisor` ≥ 1 vira `Options::tmax_divisor`. O setter troca a
camada inteira (`null`/`{}` = opções do deck) sem reabrir a sessão;
ordem das camadas: deck → `options` → `temp` (o `setTemp` vence).
`{options:{trtol:1}, tmaxDivisor:10}` é bit a bit o deck com
`.options trtol=1` e o TMAX do `.tran` a um décimo.

### Python (`spice`)

```python
s = spice.Session(deck_or_path, includes=None, options=None, tmax_divisor=None)
s.alter("r1", 2.2e3).alter("m1", "w", 4e-6).alter("vin", "pulse", [0, 1, 1e-9])
s.param("m1", "w"); s.alterable("m1")
s.temp = 85; s.warm_start = True; s.warm_stats   # WarmStats(hits, misses)
s.tmax_divisor = 10; s.tmax_divisor = None       # passos finos, sem reabrir
res = s.run()   # spice.Result, == spice.simulate(deck alterado)
```

Aceita `float`, sequências e arrays NumPy; `SpiceError(kind="alter")`;
o `run` solta o GIL.

### R (`spice`)

```r
s <- spice_session(deck, includes = NULL, options = NULL, warm_start = FALSE,
                   tmax_divisor = NULL)
s |> spice_alter("r1", 2.2e3) |> spice_alter("m1", w = 4e-6, l = 1e-6)
spice_alter(s, "vin", pulse = c(0, 1, 1e-9))
spice_set_temp(s, 85); spice_temp(s)
spice_set_tmax_divisor(s, 10); spice_tmax_divisor(s)   # NULL limpa
spice_param(s, "m1", "w"); spice_alterable(s, "m1")
res <- spice_rerun(s)   # spice_result, == spice_run(deck alterado)
```

Objeto de **referência** (handle savvy): `spice_alter()`/`spice_set_temp()`
mudam no lugar e devolvem a sessão invisível (pipe). Erros
`spice_error_alter`.

## Números (2026-11-07, Apple Silicon, release)

`cargo run --release -p dune-core --example session_speed` — mediana de
300 (30 nos decks com `.tran`) iterações alternando o valor como um
slider; "fresh" = parse + elaboração + todos os cartões:

| caso | fresh | rerun | × | rerun warm | × | warm vs frio, máx Δ/tol |
|---|---|---|---|---|---|---|
| bsim3_inv `.op` | 86 µs | 31 µs | 2.7 | 18 µs | 4.7 | 4e-13 |
| bsim3_inv `.op`+`.ac` | 221 µs | 161 µs | 1.4 | 134 µs | 1.6 | 3e-11 |
| bsim3_inv deck (`.ac .op .tran`) | 5.8 ms | 5.8 ms | 1.0 | 5.8 ms | 1.0 | 3e-11 |
| mosamp2 `.op` | 942 µs | 955 µs | 1.0 | 55 µs | 17 | 2e-10 |
| mosamp2 deck (`.tran`) | 5.3 ms | 5.0 ms | 1.06 | 5.0 ms | 1.06 | idêntico |
| mos6inv `.op` | 958 µs | 645 µs | 1.5 | 176 µs | 5.4 | 1e-5 |

(Δ/tol = |warm − frio| / (reltol·max + vntol|abstol); 1 seria o limite
do teste de convergência.)

wasm (Node, `node test/session-bench.mjs` no pacote npm):

| caso | fresh | session | × | session warm | × |
|---|---|---|---|---|---|
| rc.pic `.tran` (figura) | 12.6 ms | 0.33 ms | 38 | 0.32 ms | 39 |
| bsim3_inv `.op` | 0.17 ms | 0.056 ms | 3.0 | 0.037 ms | 4.6 |
| mosamp2 `.op` | 1.31 ms | 1.26 ms | 1.04 | 0.093 ms | 14 |

Leitura: o ganho frio vem de pular parse/elaboração (e, no Studio, a
extração pic — 97 % de um `simulatePic` pequeno); o warm start é o que
paga em circuitos não lineares cujo `.op` cai no gmin stepping
(mosamp2). Num `.tran`, a simulação domina e o rerun empata — o
`TRANOP` (~18 % do mosamp2) fica frio de propósito.

## Costura com a issue #10 (sweeps/MC)

O motor de sweeps recebe conjuntos concretos de parâmetros (D-0028).
Com a sessão, um ponto do sweep é

```rust
for set in sets { for (el, p, v) in set { s.alter(el, p, *v)?; } out.push(s.run()?); }
```

- **Paralelo:** `Session: Clone + Send` — um clone por worker (o clone
  não compartilha o histórico de warm start; `Circuit` também ganhou
  `Clone`).
- **Warm entre pontos vizinhos:** `set_warm_start(true)` dá a
  continuação do `.op` de graça; a ordem dos pontos decide a qualidade
  do chute (sweep ordenado > MC).
- **Parâmetros de modelo** (`.model` — VTO, KP…) não estão no v1; o
  lugar é o mesmo `alter` com um alvo de modelo (`alter_model("nm",
  "vto", 0.8)`), que reconstrói a temperatura de todas as instâncias do
  modelo no próximo `build` — já acontece, porque `build` roda por
  análise.

## Implementação (arquivos)

- `crates/dune-core/src/session.rs` — `AlterValue`, `Circuit::alter/
  param/alterable`, `Session`, `WarmStats`, `WarmSlot` (canal
  crate-privado `Arc<Mutex<…>>` entre a sessão e os engines construídos
  do circuito; vazio num `Circuit` comum — o `op` é o `CKTop` de
  sempre).
- `engine.rs::op` — o warm start (antes do `CKTop`, renomeado
  `op_cold`); `state.rs::len0`; `circuit.rs` — `Circuit: Clone`, campo
  `warm`, `Built::warm`.
- Gates: `crates/dune-core/tests/session.rs` (13 testes),
  `crates/dune-wasm/src/tests.rs` (3), `npm/test/smoke.mjs`,
  `python/tests/test_session.py` (6), `r/tests/testthat/test-session.R`.
