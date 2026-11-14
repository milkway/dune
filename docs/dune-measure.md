# `.measure` — medidas sobre plots calculados (issue #11)

Trilha 2 da fase além-do-SPICE (D-0028). Uma camada **pura** de
pós-processamento: dados entram (um `Solution`), escalares saem (um
`Measurement`). Nada aqui toca a simulação — o teste
`measures_do_not_touch_the_simulation` (`crates/dune-core/tests/measure.rs`)
pina que o mesmo deck com e sem `.meas` dá plots bit a bit iguais.

## Contrato

O 3f5 não tem medidas; o contrato é o **ngspice-47**
(`src/frontend/com_measure2.c` — `get_measure2`, `com_measure_when`,
`measure_at`, `measure_minMaxAvg`, `measure_rms_integral`,
`measure_parse_*` — e `measure.c` — `do_measure`, `measure_parse_line`).
Desvios deliberados estão no `corpus/KNOWN_DIFFS.md` (linhas
`.measure …`).

## A cadeia (o mesmo princípio de `dune-saida.md`)

```
.meas tran NOME FUNÇÃO …           spice.measure(plot, "FUNÇÃO …")
        │                          spice_measure(plot, "FUNÇÃO …")
        ▼                                  │
dune-netlist  ir::Measure { analysis, name, func: MeasFunc, … }
              parse_measure_body(&str) -> MeasFunc        ◄┘
        │
        ▼
dune-core     Solution::measure(&MeasFunc) -> Result<Measurement, MeasureError>
              measure_all(&[Measure], &[Solution]) -> Vec<MeasureOutcome>
        │
        ├── dune-cli   formato de saída do ngspice (measure.rs, só aqui)
        ├── dune-py    Result.measurements / spice.measure → Measurement
        ├── dune-r     res$measurements / spice_measure() → tibble
        └── dune-wasm  measurements em simulate*/Session.run (issue #14)
```

Um número, uma fonte: o valor impresso pelo CLI é o mesmo `f64` que o
Python e o R recebem.

## Gramática coberta (v1)

```
.meas[ure] {TRAN|AC|DC|SP} NOME
   TRIG|DELAY var VAL=v [TD= FROM= TO=] RISE|FALL|CROSS=n|LAST | TRIG AT=x
       TARG  var VAL=v [TD= FROM= TO=] RISE|FALL|CROSS=n|LAST | TARG AT=x
   WHEN var=v|var2 [TD= FROM= TO=] [RISE|FALL|CROSS=n|LAST] [LAST]
   FIND var AT=x
   FIND var WHEN var2=v|var3 [… como WHEN]
   DERIV[ATIVE] var AT=x | DERIV var WHEN …        (extensão Dune)
   AVG|MIN|MAX|MIN_AT|MAX_AT|PP|RMS|INTEG[RAL] var [FROM= TO=]
```

- `var` é qualquer saída do `.print` (`v(n)`, `v(a,b)`, `i(vx)`,
  `vm/vp/vr/vi/vdb(…)`, nome cru). Num plot complexo sem parte lê-se a
  parte real (o default de `get_value`); `vp` em **radianos** (o
  `radtodeg` do ngspice só converte com `units=degrees`).
- Palavras cortadas como o `gettok` (vírgula fora de parênteses
  separa); `=` com espaços em volta é colado (`val = 1`).
- RISE/FALL/CROSS: o último dado vence (`measure_parse_stdParams`);
  `LAST` solto = `CROSS=LAST`; contagem < 1 é erro (o ngspice aceita e
  produz lixo).
- `VAL` default 0 em TRIG/TARG (estrutura zerada do C).
- `TD=` em estatísticas é aceito e ignorado, como no ngspice — mas com
  aviso estruturado.
- Fora da v1, com erro estruturado e span: `PARAM=`/`EXPR` (expressões),
  `ERR*` (o ngspice também não implementa), expressões no lado direito
  de `WHEN` (`2*v(3)`, como o ngspice).

## Semântica (porte literal, com as janelas por análise)

- **Janelas.** `.tran`: `TD` (só no `WHEN`/crossings), `FROM` default
  0, `TO=0` = sem fim; o primeiro ponto além de `TO` encerra.
  `.ac`/`.sp`: idem, pontos de frequência < 0 ignorados. `.dc`:
  `FROM`/`TO` default ∓1e99, trocados se invertidos, janela inclusiva
  (a varredura pode descer); varredura aninhada que volta à origem
  reinicia a máquina de cruzamentos (zera só o contador CROSS, como o
  C).
- **Cruzamentos** (`com_measure_when`): estado acima/abaixo
  (`v < nível` → abaixo); transição abaixo→acima se `v ≥ nível`,
  acima→abaixo se `v ≤ nível`; tempo por interpolação linear com a
  fórmula do C (`px + (nível−pv)(x−px)/(v−pv)`; com dois vetores,
  `px + (pv2−pv)(x−px)/(v−pv−v2+pv2)`). `LAST` guarda o último. Um 0/0
  (trecho plano igual ao nível) é "out of interval", como lá.
- **FIND/AT** (`measure_at`): interpolação linear no segmento que
  contém `x` (também descendente no `.dc`).
- **MIN/MAX**: amostras da janela, sem interpolar as bordas; empates:
  o último vence (`<=`/`>=`).
- **AVG**: trapézios sobre as amostras da janela ÷ seu span (bordas não
  interpoladas); o `to=` reportado é o último ponto lido (quirk do C).
- **RMS/INTEG**: bordas `FROM`/`TO` interpoladas (exceto quando o ponto
  está a ≤100 ulps — `AlmostEqualUlps`), Simpson 3/8 onde há três
  larguras iguais, 1/3 onde há duas, trapézio no resto — a mesma
  escolha gulosa do C, com a mesma tolerância de ulps.
- **DERIV** (extensão): derivada de 3 pontos na malha não uniforme
  (exata em parábolas), unilateral nas pontas, interpolada como o
  `FIND`.

## Resultados (`Measurement`)

`value` é o que o ngspice imprime primeiro (e guarda como parâmetro);
`detail` (`MeasDetail`) os extras: `Delay{trig,targ}`, `At{at}`
(MIN/MAX), `With{with}` (MIN_AT/MAX_AT: `value` é a posição), `Window
{from,to}` (AVG/RMS/INTEG/PP — exatamente os números que o ngspice
imprime, inclusive o `to=0` do PP sem janela). `MeasDetail::pairs()`
achata em `(rótulo, valor)` com os rótulos do ngspice
(`trig targ at with from to`) — é a forma dos bindings.

Falhas (`MeasureError`): `OutOfInterval{clause}` (TRIG/TARG/WHEN/AT,
como o "out of interval" do ngspice), `UnknownVector`,
`UnsupportedPlot`. `describe()` dá `out of interval (WHEN)`.

## Saída do CLI (`crates/dune-cli/src/measure.rs`)

Fiel ao `out_line` do `get_measure2` e ao enquadramento do
`do_measure`: bloco `\n  Measurements for <X> Analysis\n\n`, uma linha
por medida, `\n` final; `%-20s=` e valores `%.5e`; **três** espaços
depois do `=` em `WHEN`, `RMS` e `INTEG` (quirk do `sprintf` deles);
falhas só no stderr (`Error: measure  NOME  FUNÇÃO(CLÁUSULA) : out of
interval` + `<cartão> failed!`). Antes dos `.print` (ordem do
`ngspice -b`) e também com `-r` (o ngspice recusa; KNOWN_DIFFS).
Um bloco por plot medido.

## Validação

- **Contra o ngspice, nos mesmos dados** —
  `crates/dune-core/tests/measure_ngspice.rs`: `corpus/measure/*.cir`
  (6 decks: tran RC/RLC, ac, dc, dc descendente, borda), rawfile
  **do próprio ngspice** e a saída `.meas` dele com 15 dígitos
  (`NGSPICE_MEAS_PRECISION=15`), gerados por
  `tools/measure-golden.sh` (um bloco `.control run/write` — sem ele o
  `ngspice -b` restringe os vetores salvos às medidas e perde
  `vdb(x)`). 72 medidas; pior diferença relativa 5.6e-16; régua
  `1e-14` (ordem de operações; o C imprime 16 dígitos). Fora do `--ci`
  do oráculo nesta leva.
- **Analíticos** — `crates/dune-core/tests/measure.rs`: RC (τ·ln 9,
  cruzamento 50 %, FIND, DERIV, INTEG, AVG, RMS, MAX), RLC degrau
  (pico 1+e^{−πζ/√(1−ζ²)}, período 2π/ω_d), fonte do `trrlc` (período
  10 µs exato), RC em AC (−3 dB, fase −π/4, |H|), divisor DC
  (descendente). Réguas justificadas no próprio teste.
- **CLI**: formato contra linhas capturadas do ngspice-47.

## No wasm (issue #14, dune-sim 0.3.1)

`Rawfile`/`TypedRawfile` (logo `simulate`, `simulateTyped`,
`simulatePic*`, `Session.run`/`runTyped`) carregam `measurements`: as
linhas do Python, `{name, analysis, plot, value, details, error?}`
(`MeasurementJson` em `crates/dune-wasm/src/schema.rs`), calculadas em
`Prepared::finish` — a sessão remede a cada run. Mapeamento de
ausência, fixo e documentado no `index.d.ts`:

- `value`: sempre presente; `null` quando a medida falha (o `nan` do
  Python) ou não é finita (mesma regra das `columns`).
- `details`: sempre um objeto, `{}` em falhas/`WHEN`/`FIND`/`DERIV`;
  chaves na ordem do ngspice (`trig` antes de `targ` — serialização
  própria, o mapa do serde_json ordenaria).
- `error`: **ausente** no sucesso (o `None` do Python); a linha falhou
  exatamente quando tem a chave.
- `measurements`: sempre presente, `[]` sem cartões `.measure`.

## Pendências

- `PARAM=`/`EXPR` (medidas sobre medidas; precisa de avaliador de
  expressões — o `.param` do netlist é só constante).
- ~~THD/`.four`~~: entregue como camada própria (`four`), ver
  `docs/dune-four.md`.
- Entrar no `--ci` do oráculo (comparar a saída `.meas` do ngspice
  por deck).
