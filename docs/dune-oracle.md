# dune-oracle — harness de comparação numérica

Implementa o D-0006: nada está "pronto" sem o oráculo verde. O crate roda
o ngspice fixado em `corpus/ORACLE` e compara rawfiles SPICE3 com as
tolerâncias do SPICE. Não é porte de código C; o único trecho espelhado do
3f5 é o critério de convergência que inspira a comparação (`NIconvTest`,
`src/lib/ckt/niconv.c`: `vntol` para tensões de nó, `abstol` para
correntes de ramo).

## Modos

```sh
cargo run -p dune-oracle -- --list                 # inventário do corpus
cargo run -p dune-oracle -- --ci                   # checa golden/ (sem ngspice)
cargo run -p dune-oracle -- --gen-golden [NOME..]  # gera corpus/golden/
cargo run -p dune-oracle -- --compare A.raw B.raw  # compara dois rawfiles
cargo run -p dune-oracle -- NOME..                 # ngspice fresco vs golden
```

`NOME` é o *stem* do circuito (`rc`), o caminho relativo ao `corpus/` ou o
caminho do arquivo. Qualquer FAIL encerra com código 1.

### Quem é o candidato

Para decks cujas análises **e** letras de dispositivo o motor já
implementa (inventário por varredura de linha; hoje `.op`/`.dc`/
`.tran`/`.ac` com R C L K V I E F G H D Q M X — para o M a varredura
também lê o `LEVEL` dos cartões `.model nmos/pmos`, inclusive em
linhas de continuação, e só o nível 1 entra por enquanto), o
candidato é o próprio Dune, via biblioteca
(`dune_run.rs`) — inclusive no `--ci`, que portanto exercita o motor a
cada push sem precisar de ngspice no runner. Para o resto: `NOME..`
roda o ngspice de novo (deriva de ambiente) e `--ci` compara o golden
consigo mesmo (sanidade de leitor + comparador) até a análise existir.

## Semântica da comparação

Dois valores concordam quando `|a − b| ≤ 2·(floor + reltol·max(|a|, |b|))`,
com `reltol = 1e-3` e `floor = vntol = 1e-6` para vetores `voltage`,
`abstol = 1e-12` para todo o resto (defaults do 3f5, `spice.txt`).

O fator 2 não é folga (D-0009): oráculo e candidato são dois Newtons
*independentes*, cada um parando a ≤ 1 tolerância SPICE da solução
exata; pela desigualdade triangular a distância entre eles é ≤ 2
tolerâncias. Medido na prática: a curva do diodo (`dcdiode`) chega a
margem 1,16 sem o fator — dois solvers correndo o mesmo algoritmo com
roundoff diferente. Acima de 2 tolerâncias é desacordo real.

- **Plots** são pareados em ordem e precisam ter o mesmo `Plotname`
  (case-insensitive).
- **Variáveis** alinham por nome, case-insensitive (como o `cieq` do
  frontend do 3f5). Variável do oráculo ausente no candidato → FAIL;
  variável extra no candidato → nota, não falha (o Dune pode salvar mais
  vetores que o golden).
- **Colunas** precisam ter o mesmo comprimento; `NaN` dos dois lados no
  mesmo ponto não é desacordo, `NaN`/`±inf` de um lado só é FAIL.
- **Complexos** comparam parte real e imaginária separadamente com a
  mesma fórmula.
- O relatório traz `max|err|` e `margin` = pior razão
  `|a−b| / (floor + reltol·max)`; `margin ≤ 1` é PASS.

`.tran` é comparado por interpolação linear do candidato na malha do
oráculo, com a régua derivada do LTE (D-0010): `|a−b| ≤
2·(trtol·reltol·S + floor)`, `S` = escala da forma de onda; o pior
vetor sai com erro máximo e RMS numa nota.

O `.tran` ganhou três refinamentos (D-0011): um **tubo temporal**
por ponto (um desvio é julgado contra o valor do candidato mais
próximo dentro de ±1 passo local do oráculo — dentro desse intervalo
o golden é só interpolação linear e não localiza borda nenhuma); um
**deslocamento global único** por plot, limitado ao orçamento de
fase acumulada `±2·trtol·reltol·tstop` e sempre reportado (eventos
auto-temporizados herdam a fase integrada dos dois motores); e o
veredito **decidido pelas tensões de nó** — correntes de ramo são
comparadas e reportadas, mas não decidem, porque são grandezas
diferenciais (`i(vin) = (v1−v2)/R`) cuja régua na própria escala não
é reconstruível do rawfile. `corpus/XFAIL` registra comparações
conhecidamente marginais (hoje: o transiente do `schmitt`, bordas
regenerativas) com teto de margem: rodam e aparecem em todo CI,
XFAIL dentro do teto não derruba, estourar o teto derruba, e passar
pede remoção da entrada.

Dois guardas fecham buracos dessas réguas (2026-11-08):

- **Escala como grade.** O primeiro vetor de um plot não-transiente
  que é escala (`frequency`, `v-sweep`, `i-sweep`, `res-sweep`,
  `temp-sweep`) não é solução de Newton: os dois programas o
  calculam pela mesma aritmética a partir do cartão. É comparado à
  parte com régua de arredondamento, `|a−b| ≤ 1e-9·(max(|a|,|b|) +
  max|coluna|)`, e a divergência é **problema estrutural** ("scale
  … differs at point k (grid mismatch)"), não erro de variável.
  Medido no corpus: o pior ponto fica a 1.7e-7 dessa régua (as
  grades batem a ~1e-16 relativo). Antes, a malha errada do acdec
  só aparecia como erro nos vetores, sob a régua frouxa 2·reltol.
- **Transiente: guarda pontual e shift.** O tubo de ±1 passo
  perdoa uma borda em qualquer lugar do passo do oráculo — e por
  isso perdoava também uma borda determinística escorregada um
  passo inteiro (o `tra_uic` antes do breakpoint em TSTEP sob UIC:
  0.68 V pontual, margem 0.000). Agora a razão de cada tensão é
  `max(tubo, pontual/16)`, onde "pontual" é o erro interpolado sem
  tubo na mesma régua LTE: 16 réguas ≈ `0.22·S`. Medido nos 78
  decks: todo transiente que passa fica ≤ 0.27 réguas pontuais (60×
  abaixo); o `tra_uic` pré-fix dá ~48. E um plot que só fica dentro
  da régua **depois** do deslocamento global de D-0011 não é PASS
  (`shift_needed`): nenhum deck que passa precisa de shift; com o
  guarda pontual, o `tra_uic` pré-fix era resgatado por um shift de
  −6.9e-11 s (um passo inicial) — a assinatura de uma grade
  escorregada. O shift continua buscado e reportado e a margem com
  ele ainda mede os XFAIL contra os tetos: schmitt sobe de 2.69 a
  3.02 (teto 5), ltra_3 29.9 e mosamp2 6.08 inalterados.

`.ac` compara parte real e imaginária com a régua de D-0009 — mais
apertado que o módulo-em-dB + fase que o PLAN §2.4 esboçou, e viável
porque o AC é um solve linear direto (sem folga de Newton): os três
decks do corpus batem a ~1e-9..1e-6.

## Corpus e golden

- Descoberta: `corpus/spice3f5/examples`, `corpus/berkeley`,
  `corpus/regress` (recursivo, `*.cir`). O stem precisa ser único no
  corpus — ele nomeia o golden (`corpus/golden/<stem>.raw`).
- `corpus/SKIP`: circuitos que o oráculo não roda, um por linha
  (`<caminho relativo> <motivo…>`); a história de cada um fica no
  `corpus/KNOWN_DIFFS.md`.
- `--gen-golden` exige que o ngspice instalado seja exatamente o de
  `corpus/ORACLE` e grava ASCII (`ngspice -b -D filetype=ascii -r`),
  melhor para diff no git. Todo golden do ngspice carrega o aviso
  `misplaced Command: line` do leitor — esperado, ver KNOWN_DIFFS.
- O inventário de análises do `--list` é uma varredura de linha (cartões
  `.op/.dc/.ac/.tran/.pz/.noise/.disto/.sens/.tf`); o parser de verdade é
  o `dune-netlist` (Fase 1).

## Limitações conhecidas

- Sem timeout no subprocesso ngspice (os exemplos rodam em < 1 s; vira
  necessidade quando o corpus ganhar circuitos grandes).
- `--gen-golden` sobrescreve sem perguntar — o git é a rede de proteção.

## XFAIL de 5 campos (D-0022)

`stem|plot|cap|flags|reason`; `flags` = `-` ou lista com:
`bistable` (deck bifurca por ulps — PASS e FAIL ≤ cap são ambos
esperados; PASS não pede remoção) e `accept-error=TEXT` (falha
estruturada contendo TEXT é o desfecho do outro ramo; reportada,
não falha o CI). O sweep de corpus do pacote R lê este arquivo e
aplica a mesma política.
