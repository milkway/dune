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
implementa (inventário por varredura de linha; hoje `.op`/`.dc` com
R C L K V I E F G H D X), o candidato é o próprio Dune, via biblioteca
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
