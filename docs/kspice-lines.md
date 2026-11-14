# Linhas KSPICE — TXL (`Y`) e CPL (`P`) do ngspice-47

Porte das linhas de transmissão do KSPICE no ngspice-47: a linha única
com perdas **TXL** (`src/spicelib/devices/txl/`, cartão `Y`) e as linhas
acopladas multicondutoras **CPL** (`src/spicelib/devices/cpl/`, cartão
`P`). Licença Modified BSD: "Copyright 1992 Regents of the University of
California. All rights reserved. Author: 1992 Charles Hough" (o setup do
CPL também "Modified: 2004 Paolo Nenzi"; a diagonalização de Jacobi e as
estruturas SWEC de `swec.h`/`multi_line.h` são de Shen Lin, 1990–1991,
mesmo copyright). O SPICE3f5 não tem nenhuma das duas: contrato **e**
oráculo são o ngspice-47. Não ficam atrás da feature `bsim`.

Código: `crates/dune-core/src/kspice/` (`mod.rs`, `txl.rs`, `cpl.rs`,
`cplsetup.rs`, `tests.rs`); sintaxe em `crates/dune-netlist/src/elem.rs`
(`txl`, `cpl`, `line_length`); elaboração em
`crates/dune-core/src/elaborate.rs` (`txl_model`, `cpl_model`,
`txl_instance`) e `circuit.rs` (`Spec::Txl`, `Spec::Cpl`).

## Mapa C → Rust

| C (ngspice-47) | Rust | Observação |
|---|---|---|
| `parser/inp2y.c` (`INP2Y`) | `dune-netlist::elem::txl`, `elaborate` (`Device::Txl`, `txl_instance`) | 4 nós (2 de referência, criados e não ligados); `len`; desvio para `INP2P` com modelo CPL; o fallback 3-π |
| `parser/inp2p.c` (`INP2P`) | `dune-netlist::elem::cpl` | `k = (n − 2)/2` contando os tokens até `length`/`len`; `length` ou `len` |
| `parser/inpdomod.c` (`txl`, `cpl`) | `elaborate::txl_model`, `cpl_model` | `R/L` decide TXL ou CPL |
| `txl.c`, `txlmpar.c`, `txlparam.c` | `TxlModel`, `Txl` | `R L G C LENGTH`, flag `txl`; instância `length` |
| `txlsetup.c`: `TXLsetup`, `ReadTxL` | `TxlParams::new`, `circuit.rs` (ramos), `TxlI::alloc` | ordem do `TSTALLOC` |
| `txlsetup.c`: `main_pade`, `y_pade`, `mac`, `Gaussian_Elimination1/2`, `find_roots`, `exp_pade`, `pade`, `exp_find_roots`, `get_c`, `get_h3`, `update_h1C_c`, `root3`, `div3`, `eval2`, `div_C` | funções homônimas em `txl.rs` | as estáticas de arquivo viram locais |
| `txlsetup.c`: `insert_node`, `insert_ND`, `NEW_node`, `node_tab`, `ndn_btree` | campos `in_v/in_dv/out_v/out_dv` de `TxlI` | ver "Estado global" |
| `txlload.c`: `TXLload` | `TxlI::load` (`advance`, `check_step`, `companion`, `dc_stamps`) | as duas voltas de instância do C |
| `txlload.c`: `TXLload` como `DEVacLoad` | `TxlI::load_ac` | ver "AC do TXL" |
| `txlload.c`: `copy_tx`, `update_cnv_txl`, `update_delayed_cnv_txl`, `right_consts_txl`, `get_pvs_vi_txl`, `add_new_vi_txl`, `new_vi_txl`/`free_vi_txl`, `expC`, `multC` | homônimas em `txl.rs`/`mod.rs`; `History` | a lista ligada + free list vira um `VecDeque` com índices absolutos |
| `txlacct.c` (`TXLaccept`) | — | existe no C mas **não** está ligada (`DEVaccept = NULL` em `txlinit.c`) |
| `cpl.c`, `cplmpar.c`, `cplparam.c` | `CplModel`, `Cpl` | vetores `R L G C` (triângulo superior por linhas), `LENGTH`, flags `cpl`/`txl`; instância `length` |
| `cplsetup.c`: `CPLsetup`, `ReadCpL` | `CplParams::new`, `circuit.rs`, `CplI::alloc` | |
| `cplsetup.c`: `coupled`, `loop_ZY`, `diag`, `ordering`, `reordering`, `rotate`, `sort`, `delete_1`, `eval_frequency`, `eval_Si_Si_1`, `Gaussian_Elimination2`, `store`, `store_SiSv_1`, `poly_matrix`, `match`, `polint`, `poly_W`, `approx_mode`, `matrix_p_mult`, `mult_p`, `generate_out`, `Pade_apx`, `Gaussian_Elimination`, `find_roots`, `root3`, `div3`, `get_c`, `eval2` | `cplsetup.rs` (struct `Setup` e funções homônimas) | todas as estáticas viram campos de `Setup`, novo a cada instância |
| `cplload.c`: `CPLload` | `CplI::load` (`advance`, `companion`, `dc_stamps`) | |
| `cplload.c`: `copy_cp`, `right_consts`, `update_cnv`, `update_cnv_a`, `update_delayed_cnv`, `get_pvs_vi`, `add_new_vi`, `new_vi`/`free_vi`, `divC`, `expC`, `multC` | homônimas em `cpl.rs`; `advance_tms` junta os dois laços de fontes atrasadas de `right_consts` | |
| `cplhash.c` (GC de memória) | — | o Rust libera sozinho |
| `txlask.c`, `txlmask.c`, `cplask.c`, `cplmask.c`, `*bindCSC.c`, `*delete.c` | — | consultas `@`, KLU e liberação |

## Sintaxe

```
YXXX N1 GND1 N2 GND2 MNAME <LEN VALUE>
.model MNAME txl R=… L=… G=… C=… LENGTH=…

PXXX N1 … Nk GND1 M1 … Mk GND2 MNAME <LENGTH|LEN VALUE>
.model MNAME cpl R=r11 r12 … r1k r22 … rkk  L=…  G=…  C=…  LENGTH=…
```

- Os nós de referência `GND1`, `GND2` entram na tabela de nós (como o
  `INPtermInsert` do C) mas a linha só liga `N1`/`N2` (ou `N1…Nk`,
  `M1…Mk`) contra a terra global. Um nome de referência que não seja
  `0` cria um nó sem estampa — matriz singular, nos dois simuladores.
- O `Y` só lê `len`; um `length` no cartão é ignorado pelo `INP2Y`
  (o Dune aceita e avisa). Quando o modelo é CPL, o `INP2P` relê o
  cartão e aí `length` e `len` valem.
- `inpdomod.c` decide o tipo de um modelo `txl` por `R/L` (`R` sozinho
  com `L = 0`): `≤ 1,6e9` → TXL; `1,6e9 < R/L ≤ 1,6e10` → **CPL** de um
  condutor (o `Y` vira um `P`, `INP2Y` → `INP2P`); `> 1,6e10` → TXL de
  novo (o primeiro `INPmakeMod` vence), e o `INP2Y` troca a linha pelo
  **fallback 3-π** quando há comprimento: resistores `txlres1-3<nome>`
  pelos nós `txlnd1<nome>`, `txlnd2<nome>` e capacitores `txlcap1-4<nome>`
  ao primeiro nó de referência.
- Saída: cada ramo do C (`branch1`, `branch2` do TXL; `branch1_m`,
  `branch2_m` do CPL) sai numerado na ordem de criação: `i(<nome>#1)`,
  `i(<nome>#2)`, … (D-0066). O ngspice escreve todos como `i(<nome>)`,
  repetido; nome repetido fazia os pacotes R e Python perderem ou
  misturarem séries. O comparador do oráculo pareia o r-ésimo repetido
  do ngspice com o `#r` do Dune (`paired_index`).

## Parâmetros e defaults

Nenhum parâmetro tem default: `TXLsetup`/`CPLsetup` recusam o modelo
sem `R`, `G`, `L`, `C` ou `LENGTH` ("lossy line … not given"). Ajustes
do setup:

- TXL: `L ← max(L, 1e-12)`; comprimento zero é fatal ("transmission line
  of zero length"); `R/L < 5e5` **e** `G < 1e-2` → linha sem perdas
  (`lsl`); a resistência DC é `R·len` com o `R` do modelo.
- CPL: cada entrada do triângulo de `R` é levantada a `1e-4` **no próprio
  modelo** (também as fora da diagonal), e a DC usa esse valor;
  `k ≤ 8` (`MAX_CP_TX_LINES`).

## Método

### TXL

Admitância característica `Y(s) = sqrt((G + sC)/(R + sL)) =
sqrt(C/L)·sqrt((1 + (G/C)/s)/(1 + (R/L)/s))`. `mac` dá a série de
Maclaurin de `F(z) = sqrt((1+az)/(1+bz))` (`a = G/C`, `b = R/L`, `z =
1/s`) até `z⁵`; `y_pade` resolve o Padé [3/3] (`Gaussian_Elimination1`,
pivô `1e-16`) e expande em frações parciais com três polos reais
(`find_roots`; um par complexo é fatal):

    h1(s) = sqrt(C/L) · (1 + Σ_{k=1..3} c_k/(s − x_k))

Propagação `e^{−len·sqrt((R+sL)(G+sC))} = e^{−sτ}·P(s)`, `τ =
sqrt(LC)·len`: `exp_pade` desenvolve o expoente sem o atraso
(`a0…a5`), `pade` exponencia a série e ajusta outro Padé [3/3]
(`Gaussian_Elimination2`, pivô `1e-28`), com o termo constante
`1 − e^{a0 − len·sqrt(RG)}`:

    h2(s) = e^{−a0} · (1 + Σ ec_k/(s − ex_k))    (ex2, ex3 podem ser um par complexo)

`get_h3` monta `h3 = h1·h2` (seis termos, os polos dos dois) e
`update_h1C_c` escala os resíduos: `h1·sqrt(C/L)`, `h2·e^{−a0}`,
`h3·e^{−a0}·sqrt(C/L)`; `h1C = Σ c_k` de `h1`.

Carga (equações de ramo, `i1` entra por `N1`, `i2` por `N2`):

    −i1 + (sqrt(C/L) + h/2·h1C)·v1 − [atraso]·v2 − [atraso]·i2 = ff
    −i2 + (sqrt(C/L) + h/2·h1C)·v2 − [atraso]·v1 − [atraso]·i1 = gg

`ff`, `gg` (`right_consts_txl`) somam as convoluções recursivas: cada
termo `c/(s − x)` guarda `cnv`, avançado pelo trapézio sobre o passo
`h`: `cnv ← cnv·e^{xh} + h/2·c·(u(t−h)·e^{xh} + u(t))`. As entradas
atrasadas `u(t − τ)` vêm da história por interpolação linear
(`get_pvs_vi_txl`). Quando o passo é maior que o que resta do atraso
(`t − τ > t_anterior`, `ext`), o valor em `t − τ` ainda é incógnita: a
fração `ratio = (t − τ − t_ant)/h` dele vai para a matriz (as entradas
`[atraso]`) e o resto para o lado direito. Em DC a linha é `R·len` em
série (sem `G`). Linha sem perdas: só `sqrt(C/L)` e as fontes atrasadas
`sqrt(C/L)·v(t−τ) + i(t−τ)`.

A aceitação é preguiçosa: na primeira carga de um novo ponto (`time =
(int)((t − h)·1e12)` além da última amostra) a linha de ensaio vira a
oficial, a solução aceita (`CKTrhsOld`) entra na história e as
convoluções de `h1` avançam com tensões e inclinações dos nós
(`update_cnv_txl`; `update_delayed_cnv_txl` acrescenta a parte do passo
que caiu dentro do atraso). Os tempos da história são **picossegundos
inteiros**.

### CPL

Para `k` condutores com matrizes `R, L, G, C` por metro, `coupled()`:

1. `loop_ZY(y)` com `y = 1/s`: diagonaliza `F·C + y·G` por rotações de
   Jacobi (`diag`: escala o triângulo superior por `2/(min+max)`, gira
   sempre a maior entrada fora da diagonal, até `≤ 1e-8`), forma
   `Y5 = Sv·D^{1/2}·Sv'` e diagonaliza `Y5·(F·L + y·R)·Y5`. Fica `Sv`
   (modos), `Sv_1` e `D` (autovalores).
2. `eval_frequency`: `F = 1/sqrt(min D)` e a escada `y = 0, 8·len,
   16·len, …, 56·len` (grau 7, `Left_deg`).
3. Em cada `y`: `Si_1 = Sv_1·(yR + F·L)/sqrt(D)`, `Si = Si_1⁻¹`
   (`Gaussian_Elimination2` invertendo `[A | I]`), `SiSv_1 = Si·Sv_1`.
4. `match`/`polint` (Neville extrapolado a 0) ajusta polinômios de grau
   7 em `y` a cada entrada; `approx_mode` tira de cada autovalor o
   atraso modal `τ_i = sqrt(W0)·len/F` e a série de atenuação;
   `matrix_p_mult` forma, por modo `k`, `IWI = Si·diag(W)·Si_1` e
   `IWV = Si·diag(W)·Sv_1`, normalizados a termo constante 1.
5. `generate_out` ajusta Padé [3/3] (`Pade_apx`, pivô `1e-28`) a
   `SiSv_1` (admitância, `h1t`), `IWI` (`h2t`, corrente) e `IWV` (`h3t`,
   tensão); o termo constante das diagonais é
   `sqrt(G/R)` e `e^{−sqrt(GR)·len}`. Um par de polos complexos é
   guardado como (real, imaginário) com `ifImg`.

A carga é a do TXL com matrizes: por condutor `m`, `−i1_m + Σ_p
(h1t[m][p].aten + h/2·h1C[m][p])·v1_p = ff_m`, as fontes atrasadas por
modo (`τ_l`) e por condutor.

## Estado global e como foi reestruturado

| No C | No Dune | Por quê é equivalente / o que muda |
|---|---|---|
| `node_tab`, `ndn_btree` (TXL, globais) e as do CPL (`static`): um `NODE` por nome de nó, com `V`, `dv`, compartilhado por todas as linhas que tocam o nó | `in_v/in_dv/out_v/out_dv` por instância | toda linha escreve no `NODE` os valores da sua última amostra aceita, que é `CKTrhsOld` do nó no mesmo instante; como todas as linhas acrescentam amostras nos mesmos instantes, o valor lido é o mesmo, e a ordem de carga deixa de importar |
| `int time` compartilhado entre instâncias no `TXLload`/`CPLload` (o "pulo" para a última amostra quando o arredondamento o deixou atrás) | `time` por instância: `max(time, cauda própria)` | as caudas de todas as linhas coincidem, logo o máximo acumulado do C é o mesmo |
| `ckt->CKTmaxStep = 0.9·τ` escrito de dentro da carga (a última linha que disparar vence) | `Device::max_step` pegajoso por instância; o transiente usa o mínimo (`Engine::device_max_step`) | o mesmo quando uma linha só dispara; com várias de atrasos diferentes o C oscila até o menor, o Dune vai direto a ele (`KNOWN_DIFFS.md`) |
| `static double ratio[MAX_CP_TX_LINES]` | valor de retorno de `get_pvs` | usado só logo depois |
| `pool_vi_txl`, `pool_vi` (free lists globais) | `VecDeque` por instância, podado quando as duas linhas passaram da amostra | sem efeito numérico |
| estáticas de `txlsetup.c` (`sqtCdL`, `b1…b5`, `p*`, `q*`, `c*`, `x*`, `A`, `RG`, `tau`, `a0…a5`, `ep*`, `eq*`, `ec*`, `ex*`, `ifImg`, `AA`) | locais de `y_pade`/`exp_pade` | escritas antes de lidas em cada setup |
| estáticas de `cplsetup.c` (`ZY`, `Sv`, `D`, `Y5`, `Y5_1`, `Sv_1`, `R_m…C_m`, `length`, `TAU`, `A`, `frequency`, `Si`, `Si_1`, `SiSv_1`, `Sip`, `Si_1p`, `Sv_1p`, `W`, `IWI`, `IWV`, `SIV`, `At`, `Scaling_F`, `Scaling_F2`, `dim`, `row`) | campos de `cplsetup::Setup`, criado por instância | escritos antes de lidos, **exceto** as linhas `dims…2·dims` de `A` (ver Fidelidade) |
| `TXLdcGiven`/`CPLdcGiven`, zerados só no `*unsetup` | `restart()` a cada carga DC | toda análise do Dune carrega o seu ponto de operação antes da primeira carga não-DC; o par de linhas, a história, o teto de passo e a falha voltam ao estado do setup |
| `R_m[i][j] = Rm[c] = MAX(f, 1e-4)` escreve no modelo | clamp numa cópia em `CplParams` | idempotente |

A carga paralela (`load_threads`) fica bit a bit: as duas linhas só
tocam os próprios campos e o sumidouro de estampas (`par_load = true`).

## No perfil Dune (TR-BDF2)

O C só conhece um passo por ponto: a aceitação preguiçosa da carga
(primeira carga de um ponto novo ⇒ a amostra anterior entra na
história) vê sempre um ponto aceito. O TR-BDF2 do perfil Dune resolve um
estágio intermediário em `t + γh` sem aceitá-lo, e a carga do estágio
final o acrescenta como amostra; se o controle de erro rejeita o passo,
essa amostra ficaria na história. Por isso as duas linhas implementam
`Device::accept` (que o C não liga): uma amostra acrescentada sem
aceitação desde a anterior é **provisória** — guarda-se o estado de
antes (linha oficial, fim da história, nós) e, se a próxima carga vier
com um tempo anterior à cauda, ela é desfeita. Com o trapézio e o Gear
nenhuma amostra é provisória, e o comportamento é o do C. Medido: o
deck de teste `trbdf2_stage_samples_roll_back_with_a_rejected_step`
desfaz quatro amostras e fica a < 0,05 V da corrida trapezoidal; os
decks do corpus rodam em `compat=spice3`.

## AC do TXL

`txlinit.c` instala `TXLload` como `DEVacLoad`. Numa análise AC o
ngspice tem `CKTtime = CKTdelta = 0` e `CKTmode = MODEAC` (não DC):
a primeira carga AC faz o dc setup com `CKTrhsOld` = ponto de operação
(o Dune o guarda na passada `MODEINITSMSIG`, que tem o mesmo
`CKTrhsOld`), e cada frequência estampa o companheiro transiente com
`h = 0`: `sqrt(C/L)` de cada extremidade à terra, extremidades
desacopladas, e `ff`/`gg` — calculados do ponto de operação — no lado
direito **real**, como excitação constante em toda frequência. As
estampas vão na parte real da matriz complexa (o ponteiro do C aponta
para `Real`). Medido: `regress/txl_ac.cir` (1 V de polarização) bate
com o ngspice a 2e-16.

## Análises

| Análise | TXL | CPL |
|---|---|---|
| `.op`, `.dc`, `.tf` | carga DC | carga DC |
| `.tran` | sim | sim (com `.options compat=spice3` nos decks) |
| `.ac`, `.noise` | `TXLload` como carga AC (acima); sem ruído próprio | **recusado**: o C não tem `DEVacLoad`; no ngspice os ramos ficam sem estampa e o `.ac` para com "matrix is singular" (medido) |
| `.sp` | a mesma carga AC (o `.sp` só lê a parte imaginária do RHS carregado, então a excitação constante some); não sondado | recusado |
| `.pz` | recusado (sem `DEVpzLoad`; o C pula o dispositivo em silêncio) | recusado |
| `.disto` | recusado (sem `DEVdisto`; o C usaria a carga AC com o RHS espúrio) | recusado |
| `.sens` | recusado (sem `DEVsen*`) | recusado |
| `.pss` | recusado (a história parte de um ponto de operação; sem passado periódico) | recusado |

## Fidelidade: o que o C faz e o Dune repete

Portados literalmente (prováveis bugs ou esquisitices do C):

1. **Inclinações acumuladas** (`update_cnv_txl`, `update_cnv` do CPL):
   `bi *= t; bo *= t` dentro do laço dos três termos — a inclinação do
   termo 2 sai multiplicada por `c1/x1·c2/x2`, a do 3 por três fatores.
   A fórmula pede `t` só do termo corrente.
2. **AC do TXL** = carga transiente com `h = 0` (seção acima): uma
   excitação real constante no `.ac` de uma linha polarizada.
3. **`0,1·gmin` nas quatro entradas** `pos-pos`, `neg-neg`, `pos-neg`,
   `neg-pos`, todas com sinal `+` (não é uma condutância entre os nós),
   em todo modo, TXL e cada condutor do CPL.
4. **Atribuição em vez de soma** (`*ptr = v`) nas linhas de ramo: o
   Dune guarda cada estampa num `SetBuf` com a regra "a última
   atribuição vence" e soma o buffer no fim, o que dá o mesmo resultado
   inclusive quando dois ponteiros apontam para a mesma posição (linha
   com as duas pontas no mesmo nó).
5. **CPL: estampas atrasadas por modo sobrescritas**: para cada modo `q`
   com `ratio[q] > 0` o C *atribui* `ibr1Neg[m][p] = −ratio[q]·(…)`, de
   modo que fica só o último modo — não a soma dos modos.
6. **Atraso de uma iteração em `ext`/`ratio`**: `copy_tx(tx2, tx)` no
   começo da segunda volta e `copy_tx(tx, tx2)` na aceitação fazem a
   linha oficial herdar `ext`/`ratio` da penúltima iteração de Newton
   do ponto aceito (no CPL, `cp2->ext` e `cp->ratio`, ao contrário do
   TXL). Como toda iteração de um ponto vê o mesmo `t` e `h`, é
   observável só se um ponto convergisse em uma iteração (o `NIiter`
   exige duas).
7. **Termo 4 contado duas vezes** no ramo `ifImg` do TXL
   (`2·Re` do par; o termo 5 guarda a parte imaginária), e
   `update_delayed_cnv_txl` trata os seis termos — partes imaginárias
   incluídas — como reais.
8. **Fallback 3-π**: resistência `3/(R·len)` em cada um dos três
   resistores, o inverso de um terço da resistência da linha
   (`R·len/3`); a capacitância está certa. Medido em `regress/txl_3pi.cir`
   (R = 500 Ω/m, len = 0,05 m: 0,12 Ω em vez de 8,3 Ω).
9. **`Y` ignora `length`**; só `len`.
10. **Picossegundos inteiros**: `(int)(t·1e12)`. A conversão do C é
    indefinida fora de `int` (≈ 2,147 ms); o Dune satura, como o arm64
    faz. E a truncagem amplia diferenças de 1 ulp no tempo: em
    `t = 2,53 ns` o ngspice tinha `2,53e-9` (→ 2530 ps) e o Dune
    `2,5299999999999997e-9` (→ 2529 ps) num deck de sondagem, e o
    ponto difere em 1e-3 V (dentro da régua). Os decks põem bordas e
    `TMAX` fora de picossegundos inteiros para não morar nessa borda.
11. **`TXLaccept` não ligado**: as linhas não plantam breakpoints nem
    limitam o passo por erro de truncamento; só o teto `0,9·τ`.
12. **CPL: teto testado também em DC** (`mintaul < hf` antes do
    `if (cond1)`; inofensivo, `hf = 0`), com sentinela `123456789` ps.
13. **CPL: clamp de `R` também fora da diagonal** (um `0` vira `1e-4`
    Ω/m), e a DC usa o valor levantado.
14. **CPL: correntes iniciais zero** na história do dc setup (o TXL usa
    as correntes de ramo do ponto de operação).
15. **CPL: cabeça da história** só avança pelo modo de menor `ta` (o mais
    lento); os demais modos buscam a partir dela.
16. **`ReadCpL` devolve −1** quando um atraso é menor que `CKTmaxStep`,
    e o `CPLsetup` ignora o retorno: sem efeito, não portado.
17. **Ajuste de propagação instável**: quando `exp_find_roots` acha um
    par complexo, o par tem parte real positiva em 5 de 6 casos de uma
    grade (`R ∈ {0,5; 5; 14}`, `G ∈ {0,02; 1}`, `len ∈ {1; 10}`, `L =
    8,972e-9`, `C = 0,468e-12`): `R=0,5 G=0,02 len=10` dá `+3,1e9 1/s`
    e `v(b)` chega a 2e19 V aos 18 ns **no ngspice** (o Dune segue);
    `R=5 G=1 len=1` dá `+6,1e12 1/s`: o ngspice para com "timestep too
    small" em 0,403 ns, o Dune com "singular matrix" (o NaN chega à
    fatoração). O caso estável (`R=14 G=0,02 len=10`, `−7,9e7 1/s`) é
    `regress/txl_img.cir`, com `L` e `C` dez vezes maiores (o ajuste
    só depende de `R/L·τ` e `G/C·τ`, que não mudam; o polo fica em
    `−7,9e6 1/s` e o atraso em 6,5 ns, na escala da régua).
18. **CPL desacoplado** (mútuas `L` e `C` nulas): ajustes com termo
    constante zero ficam `NULL`, e a primeira carga transiente sai com
    "Forbidden combination of model parameters"; a DC funciona. O Dune
    falha a análise com a mesma mensagem (`Device::fatal`).
19. **Pivôs da inversão do CPL** (`Gaussian_Elimination2(dims, −1)`):
    a busca de pivô corre `2·dims` linhas da estática `A`, cujas linhas
    `dims…2·dims` nunca são escritas — zeros, a menos que uma CPL
    **maior** tenha sido montada antes no mesmo processo e deixado as
    suas linhas lá, que então podem ser escolhidas como pivô. O Dune
    zera `A` a cada instância (o caso "primeira CPL"), e documenta o
    resto em `KNOWN_DIFFS.md`.
20. A detecção de `R` do `inpdomod.c` aceita qualquer token que comece
    com `r`; o Dune lê só `r`.

## Sondas (`corpus/regress/txl_*.cir`, `cpl_*.cir`, goldens do ngspice-47)

| Deck | Cobre |
|---|---|
| `txl_matched` | TXL com perdas, terminações quase casadas, pulso |
| `txl_mismatch` | linha curta descasada; o teto `0,9·τ` vindo da carga; `len=` |
| `txl_lossless` | `lsl`, reflexões com carga capacitiva |
| `txl_img` | par complexo estável no ajuste de propagação |
| `txl_dc` | `.dc` por duas linhas em cascata, `len` sobre `length` |
| `txl_ac` | a carga AC do TXL com polarização DC, linha com e sem perdas |
| `txl_cpl` | cartão `txl` virando CPL de um condutor pelo `R/L` |
| `txl_3pi` | o fallback 3-π |
| `cpl_2line` | 2 condutores, diafonia próxima e distante |
| `cpl_3line` | 3 condutores com `R` e `G` acoplados, `length` da instância |
| `cpl_op` | `.op` com CPL de 3 e de 2 condutores e um TXL |

## Pendências

- `.sp` com TXL aceito e não sondado.
- O fallback 3-π com o segundo nó de referência diferente de `0`: o C
  cria esse nó (sem estampa, singular); o Dune não o cria.
