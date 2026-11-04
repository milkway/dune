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

### Escopo da Fase 0

O `dune-cli` ainda não simula, então:

- `NOME..` roda o ngspice de novo e compara com o golden versionado —
  detecta deriva de ambiente/versão, e é o esqueleto do fluxo definitivo;
- `--ci` compara cada golden **consigo mesmo** — sanidade de leitor +
  comparador sem precisar de ngspice no runner de CI.

Da Fase 1 em diante o lado "candidato" passa a ser a saída do Dune.

## Semântica da comparação

Dois valores concordam quando `|a − b| ≤ floor + reltol·max(|a|, |b|)`,
com `reltol = 1e-3` e `floor = vntol = 1e-6` para vetores `voltage`,
`abstol = 1e-12` para todo o resto (defaults do 3f5, `spice.txt`).

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

Pendências deliberadas (Fase 2, PLAN.md §2.4): interpolar `.tran` do
candidato na malha de tempo do oráculo (hoje malha diferente → FAIL
estrutural) e comparar `.ac` como módulo em dB + fase (hoje re/im).

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
