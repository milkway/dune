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

## XFAILs e os decks-companheiros convergidos (2026-11-11)

Os quatro XFAILs não são defeitos do motor: são decks em que o passo
default deixa o resultado sensível ao último bit, e o golden é UMA
amostra do ngspice. Cada um tem agora um companheiro em
`corpus/regress/` — o mesmo circuito de Berkeley, num regime em que a
resposta é única — que **tem de passar** no `--ci`:

| XFAIL | por que falha no default | companheiro | margem |
|---|---|---|---|
| `schmitt` | bordas regenerativas, fase do ringing do trapézio | `schmitt_fine` (TMAX = 1 ns) | 0,004 |
| `mosamp2` | controlador LTE biestável (cargas de Meyer) | `mosamp2_fine` (TMAX = 10 ns) | 0,05 |
| `ltra_1` | ringing do trapézio na fileira de diodos | `ltra_1_gear` (gear, TMAX = 0,02 ns) | 0,09 |
| `ltra_3` | idem, mais cascata de breakpoints | `ltra_3_gear` (gear, TMAX = 0,02 ns) | 0,07 |

Medidas que definiram a escolha (arm64, ngspice-47):

- `schmitt` e `mosamp2` convergem com o passo: margem 0,004 em
  TMAX = 1/0,2/0,05 ns; 0,07/0,003/0,02 em 10/2/0,5 ns.
- Nos dois decks de linha o **trapézio não converge com o passo**: o
  `ltra_1` passa a 0,04/0,08/0,10 em TMAX = 0,1/0,05/0,02 ns e FALHA a
  7,8 em 0,01 ns; o `ltra_3` fica em 39–47 em qualquer passo. O
  ringing é do método (rs + carga de difusão, sem capacitância de
  junção no nó), não do passo.
- Com `method=gear` nos dois motores e o mesmo TMAX, Dune e ngspice
  coincidem (0,09–0,25). O próprio ngspice-gear entre TMAX = 0,02 e
  0,01 ns difere de si mesmo em ~1 régua: o companheiro prova "mesmo
  método, mesmo passo, mesma resposta", não a convergência da física.
- Mesmo veredito no backend esparso (`solver-xingo`).

## Régua de precisão (`--accuracy`, 2026-11-11)

O `--ci` mede **fidelidade** (Dune contra UMA execução do ngspice nas
mesmas configurações). Ele não distingue execução precisa de
imprecisa: um deck com passo default grosso passa quando os dois
motores erram igual (`csw_rly`) e falha quando erram diferente (os
XFAILs). O `dune-oracle --accuracy` mede a outra coisa, sem ngspice:

- **candidato**: o deck como escrito;
- **referência**: o mesmo deck com `TMAX/40`, rodado pelo próprio
  Dune (que o Dune refinado coincide com o ngspice refinado é o que
  os companheiros acima e o levantamento abaixo estabelecem);
- a referência só vale se convergiu: `TMAX/20` tem de estar a ≤ 0,2
  régua dela; senão o deck entra como `unmeasured` em
  `corpus/ACCURACY`;
- **margem**: pior razão do comparador com a grade do candidato
  dirigindo — o erro nos pontos que o motor calculou.

`corpus/ACCURACY` (`stem|teto|motivo` ou `stem|unmeasured|motivo`)
dá o teto de quem passa de 0,2; estourar o teto derruba o CI.

| default do `.tran` | decks em ≤ 0,2 | soma das margens | `schmitt` | `mosamp2` |
|---|---|---|---|---|
| trapézio (até D-0046) | 25 de 34 | 10,16 | 3,07 | 1,32 |
| TR-BDF2 (D-0046) | 31 de 34 | 4,33 | 0,10 | 0,45 |
| + eventos de chave localizados (D-0047) | 33 de 35 | 1,83 | 0,10 | 0,45 |
| + TR-BDF2 controlado por `TRTOL/7` (D-0049) | 34 de 35 | 1,17 | 0,12 | 0,08 |

Na linha da D-0047 o `sw_hyst` vai de 2,54 a 0,000 (antes os
limiares da chave eram cruzados dentro de um passo, qualquer que
fosse o método) e o `csw_rly` entra na conta (0,04; antes a
referência refinada não convergia). Na linha da D-0049 só o
`ltrafit` (0,25) fica fora de 0,2, e a régua passa a mostrar os pontos
de tempo de cada deck.

**Leitura da régua (D-0053).** Três regras próprias, distintas do
cheque de fidelidade contra o ngspice:
- *tempo de evento*: o tubo de tempo de um ponto em `t` tem pelo
  menos `RELTOL·t`, e a guarda pontual lê o erro por esse tubo — um
  salto (corte de diodo, troca de chave) que duas corridas põem a 1 ps
  uma da outra depois de 16 ns é a mesma resposta;
- *convergência na resolução do candidato*: TMAX/20 e TMAX/40 são
  comparados com o tubo alargado ao passo local do candidato;
- *margem estável*: se ainda assim diferem em mais de 0,2, a
  referência vale quando a margem muda menos de 10 % trocando uma
  pela outra.
Com isso os `ltra_*` têm número: no passo do deck, com a interpolação
quadrática do 3f5, `ltra_1` fica a 5,1 réguas e `ltra_3` a 7,3 (margem
estável); com a linear do perfil Dune (D-0052), 2,4 e 2,9 — aí a
margem oscila mais de 10 % entre as referências (o corte dos diodos
deixa um pico numa ou noutra) e os decks seguem `unmeasured`, com os
números no motivo. `ltra_2`: 1,2 ± 0,4.

Com a leitura nova, o deck `bsrc_step` e os defaults da D-0052 a régua
mede 36 decks, soma **0,96**, 19 378 pontos (os 1,17 de antes eram com
a leitura antiga, sobre 35 decks). É o número a bater em qualquer mudança de integração ou
de controle de passo. Custo do TR-BDF2 no `dune -b` sobre os 37 decks
transientes (release, melhor de 3): mediana ×1,12 por deck, pior
×1,7 (`ltra_1`, `ltra_3`), total ×0,82 (o `ltra_2`, que domina, fica
mais rápido).

**Levantamento que calibrou a régua** (ngspice-47 como referência,
`TMAX/40`, trap; 29 decks com referência convergida e Dune-refinado
≡ ngspice-refinado):

| execução no passo default | mediana | pior | > 1 | soma |
|---|---|---|---|---|
| ngspice trapézio | 0,013 | 4,54 | 2 | 9,20 |
| Dune trapézio | 0,013 | 3,07 | 2 | 6,49 |
| Dune gear | 0,055 | 1,45 | 1 | 4,97 |
| Dune trbdf2 | 0,008 | 0,41 | 0 | 1,27 |

`trbdf2` contra trapézio no Dune: melhor em 16, igual em 13, pior em
nenhum; passos aceitos ×0,98 (dois Newtons por passo). Gear piora 17.
Script e dados brutos do levantamento: `.claude/scratch/accuracy/`
(local, não versionado).

