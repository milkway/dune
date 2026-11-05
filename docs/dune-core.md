# dune-core — notas de porte (Fase 1: `.op`)

Contrato: o C em `corpus/spice3f5/src/lib/{ckt,ni,dev,sparse}`.
API pública conforme D-0008 (specs tipados + `from_netlist` +
resultados espelhando o rawfile).

## O que foi portado fielmente

- **Constantes** (`consts.rs` ← `include/const.h`): `CHARGE`,
  `CONSTboltz`, `REFTEMP` etc., bit a bit.
- **Defaults de opções** (`options.rs` ← `cktinit.c`/`cktntask.c`):
  `reltol 1e-3`, `abstol 1e-12`, `vntol 1e-6`, `gmin 1e-12`,
  `itl1 100`, `gminsteps/srcsteps 10`, `temp/tnom 300.15 K`;
  `temp`/`tnom` chegam em °C no cartão.
- **Laço de Newton** (`engine.rs` ← `ni/niiter.c`): estados
  `INITJCT → INITFIX → INITFLOAT`, troca `rhs`/`rhsOld` por iteração,
  convergência exige `iterno != 1`, o passe extra `ipass` quando há
  `.nodeset`. Convergência (`ni/niconv.c`): `vntol` para tensões de
  nó, `abstol` para correntes de ramo, ambos com o termo `reltol`.
- **`CKTop`** (`ckt/cktop.c`): NR direto com `itl1`; gmin stepping
  (`diagGmin = gmin·10^n`, dividido por 10 a cada passo convergido,
  passo final com gmin 0 **sempre** tentado, como no C); source
  stepping (`srcFact = i/n` escalando V e I independentes).
- **Stamps** dos lineares (`devices.rs` ← `dev/*/..load.c`): R, C
  (aberto em DC), L (fonte de 0 V com corrente de ramo), K (sem efeito
  DC), V (dc·srcFact), I, E, G, F, H — posições e sinais do C.
- **Diodo** (`diode.rs` ← `dioload.c`, `diotemp.c`, `devsup.c`):
  regiões exp/polinomial/ruptura com `gmin`, `DEVpnjlim` literal,
  limitação separada na ruptura, teste de convergência por corrente
  prevista (`cdhat`), nó interno `nome#internal` quando `RS≠0`,
  ajuste de temperatura de `IS` (EG/XTI) e o casamento iterativo de
  `BV`/`IBV` (25 iterações, mesmos avisos).
- **Gmin na diagonal** (`linalg` ← `spsmp.c::LoadGmin`): só posições
  diagonais presentes no padrão recebem `diagGmin`.
- **Saída**: espelha o rawfile de `.op` do ngspice — `v(<nó>)` para
  todo nó não-terra não-interno (`#` fica de fora, como no ngspice),
  `i(<nome>)` para ramos de V, L, E, H; `Plotname: Operating Point`.

- **`.tran`** (`tran.rs`, `engine.rs::tran` ← `ckt/dctran.c`): laço
  de tempo completo — TRANOP (com `.ic` forçado, ou UIC com uma única
  carga sem solve, `niiter.c`), rotação de estados (`CKTstates`),
  coeficientes trapezoidal/Gear 1–6 em grade não uniforme
  (`nicomcof.c`, com o pequeno LU), companheiros via `NIintegrate`,
  LTE por diferenças divididas (`cktterr.c`) com crescimento ≤ 2×
  (`ckttrunc.c`), controle de ordem 1↔2 do `dctran`, rejeição por
  não-convergência (δ/8) e por LTE (0.9·δ), tabela de breakpoints
  ordenada com `minBreak` (`cktsetbk.c`/`cktclrbk.c`), breakpoints de
  PULSE/PWL no accept (`vsrcacct.c`), fontes `pulse sin exp pwl sffm`
  com os defaults dependentes de TSTEP/TSTOP (`vsrcload.c`, incluindo
  o π de 3.141592654 só no SFFM). Capacitor (`capload.c`), indutor em
  duas fases com acoplamento K (`indload.c`, fluxo→múto→integra),
  capacitância do diodo (depleção + difusão com `f2`/`f3`) e seu
  `DIOtrunc`.

- **BJT Gummel–Poon completo** (`bjt.rs` ← `bjtload.c`,
  `bjttemp.c`, `bjtsetup.c`, `bjtacld.c`, `bjttrunc.c`): junções B-E
  e B-C com fugas (`ISE/ISC`), Early (`VAF/VAR`), roll-off de alta
  injeção (`IKF/IKR`, `qb` com `sqarg`), resistência de base
  dependente de corrente (`IRB`, com a fórmula da tangente),
  `DEVpnjlim` nas duas junções, quatro cargas (B-E, B-C interna,
  B-X externa via `XCJC`, C-S) com as formas `f2/f3/f6/f7`, TF
  dependente de polarização (`XTF/VTF/ITF`), fase em excesso de Weil
  (`PTF`) com Euler regressivo no transiente e `sin/cos` no AC, nós
  internos `q1#collector/base/emitter`, NPN/PNP via `type`, ajuste
  completo de temperatura. 21 estados, 23 posições de matriz, LTE em
  `qbe/qbc/qcs`. Valida contra os decks de Berkeley: `diffpair`,
  `rtlinv`, `rca3040` (ac+dc+tran) PASS; `schmitt` em
  `corpus/XFAIL` (D-0011).

- **MOSFET nível 1** (`mos1.rs` ← `mos1load.c`, `mos1temp.c`,
  `mos1set.c`, `mos1acld.c`, `mos1trunc.c`, `devsup.c`): corrente de
  dreno Shichman–Hodges (corte/saturação/linear com `LAMBDA`),
  limitação `DEVfetlim`/`DEVlimvds`/`DEVpnjlim` na ordem exata do C
  (incluindo o modo invertido com `vgd`), diodos de corpo com
  `MAX_EXP_ARG`, carga de depleção B-D/B-S com o atalho de `sqrt`
  para `MJ=0.5` e a continuação linearizada `f2/f3/f4` acima de
  `FC·PB` (CAPZEROBYPASS mantido), capacitâncias de porta de Meyer
  (`DEVqmeyer`, metade não constante no estado + média com o passo
  anterior + overlap constante), preditor de `vbs/vgs/vds` e das
  cargas `qgs/qgd/qgb` por extrapolação, pré-processamento completo
  do `MOS1temp` (KP de `U0·COX`, PHI/GAMMA/VTO de `NSUB/NSS/TPG` com
  o erro fatal `Nsub < Ni`, `Vcrit` por lado, coeficientes de
  junção por área/perímetro), nós internos `m1#drain`/`m1#source`
  quando `RD`/`RSH` existem, defaults de geometria por
  `.options defl/defw/defad/defas`. 17 estados, 22 posições de
  matriz, LTE em `qgs/qgd/qgb`. Valida contra `mosmem` (tran,
  margem 0.022, shift 0). O gate do oráculo lê o `LEVEL` dos
  cartões `.model` (níveis 2/3/6 seguem golden-vs-golden).

- **MOSFET nível 2** (`mos2.rs` ← `mos2load.c`, `mos2temp.c`,
  `mos2set.c`, `mos2acld.c`, `mos2trunc.c`): Grove-Frohman completo —
  correção de canal curto pela profundidade de junção (`XJ`),
  estreitamento (`DELTA`), degradação de mobilidade
  (`UCRIT`/`UEXP`), saturação de velocidade pela quártica de Baum
  (`VMAX`/`NEFF`, com as tabelas `sig1`/`sig2` e o caso `p<0` por
  `cos/atan`), modulação de comprimento de canal pela aproximação de
  depleção (com o limite de punch-through `xwb`), e a região de
  sublimiar exponencial (`NFS`/`xn`). O andaime (escada de
  polarização, limitação, diodos de corpo, cargas de depleção e
  Meyer, companheiros, stamps, 17 estados, 22 posições) é idêntico
  ao do MOS1. `TOX` default 1e-7; `MJSW` default 0.33; `CJ`
  defaultado de `NSUB` continua fora de `czbd` (quirk de given-ness
  do 3f5 mantido); `ueff` usa o `U0` cru do modelo, não o ajustado
  por temperatura (quirk mantido). Validação: op/`.dc`/`.ac` de um
  transistor isolado batem com o ngspice-47 a 1e-5; o tran do
  mosamp2 é XFAIL documentado (dependência de caminho do modelo de
  Meyer — ver `corpus/XFAIL`).

- **MOSFET nível 6** (`mos6.rs` ← `mos6load.c`, `mos6temp.c`,
  `mos6set.c`, `mos6trunc.c`; Sakurai–Newton): corrente de saturação
  `KC·(vgs−von)^NC`, tensão de saturação `KV·(vgs−von)^NV`, correção
  linear `(2−vdst)·vdst` abaixo de `vdsat`, limiar com
  `GAMMA·sqrt(φ−vbs) − GAMMA1·vbs − SIGMA·vds` (o 3f5 mata o termo
  SIGMA com um `;` perdido; o ngspice corrigiu e o Dune segue o
  ngspice — KNOWN_DIFFS), `LAMBDA0−LAMBDA1·vbs`, `KC` default
  `0.5·U0·COX`, alias `LAMBDA`→`LAMBDA0`, `vonbm` zerado onde o 3f5
  lê lixo (`sarg≤0`), `NVTH`/`PS` aceitos e ignorados como no 3f5.
  Sem carga AC (o 3f5 não tem `mos6acld.c`). Mesmo andaime do MOS1.
  Valida: `mos6inv` (4 inversores NAND/NOR em subcircuitos, 44 MOS)
  PASS com máx 1.3 mV. Parâmetro de modelo desconhecido agora é
  aviso ("unrecognized parameter - ignored", como o `INPmakeMod`),
  não erro — os decks do 3f5 carregam XJ em cartões nível 1/6 que as
  tabelas desses níveis nem listam; e cartões de nível não suportado
  só viram erro quando algum M os instancia (mos6inv traz cartões
  nível 3 sem uso).

- **`.ac`** (`ac.rs`, `engine.rs::ac` ← `ckt/acan.c`,
  `ni/niaciter.c`, `dev/*acld.c`): ponto de operação, um passe
  `MODEINITSMSIG` (o diodo guarda `capd` em `capCurrent`), e um
  solve complexo direto por frequência (`G + jB`, sem iteração).
  Varredura dec/oct/lin com o `freqTol` do C — incluindo o quirk de
  `lin` com 2 pontos render 1 (delta = HUGE). Stamps: R/E/F/G/H
  reais; C `+jωC`; L `−jωL` no ramo; K `−jωM`; fonte V com
  `mag·e^{jfase°}`; diodo `gd + jω·capd` do ponto de operação.
  Fase do `ACPHASE` em graus (`vsrctemp.c`).

- **`.dc`** (`dc.rs`, `engine.rs::dc` ← `ckt/dctrcurv.c`): uma ou
  duas fontes V/I, a primeira no laço interno; cada ponto tenta
  `NIiter(itl2)` continuando da solução anterior e cai para o `CKTop`
  completo; fonte interna volta ao início (e re-inicializa junções)
  quando a externa avança; escala = valor da fonte interna, vetor
  `v(v-sweep)`/`i(i-sweep)` como o ngspice, plot `DC transfer
  characteristic`.

- **`.pz`** (`pz.rs`, `engine.rs::pz_setup/pz_det` ← `ckt/pzan.c`,
  `cktpzset.c`, `cktpzld.c`, `cktpzstr.c`, `ni/nipzmeth.c`): polos e
  zeros como raízes do determinante da MNA complexa após a cirurgia
  de colunas do `CKTpzSetup` (coluna-solução zerada, coluna-balanço
  somada, entradas de excitação ±1). A busca é o "zeroin" do 3f5
  intacto: estratégias SHIFT/SKIP/SPLIT/GUESS no eixo real, SYM2
  (série quadrática em aritmética bloco-normalizada mantissa ×
  2^expoente — `zaddeq`/`R_NORM`/`C_NORM` literais), Muller para
  pares complexos semeados pelo radical do mínimo (`NIpzK`),
  deflação multiplicativa das raízes achadas, `alter` para
  desgrudar de pontos repetidos e o registro ordenado de trials
  (lista ligada por índices). Determinante via LU complexa própria
  do backend (`det_complex`, mantissa/expoente base 2 como o
  `SMPcDProd`); matriz singular = raiz. Cargas `pzLoad` para
  R C L K V I E F G H e diodo (`G + s·C`); dispositivo sem porte
  falha alto (o C pula silenciosamente). Bugs do C tratados: o
  `b = 0.0 ||` do `NIpzSym2` (atribuição dentro da condição)
  portado literal; `vonbm`/`new_mag` não inicializados viram 0;
  o `abort()` do Strat e o "Lost numerical stability" viram
  fallback de ponto médio. Valida: `simplepz`, `pz2`, `pzt` PASS
  contra o ngspice-47 a ≤1.5e-8.

- **`.noise`** (`noise.rs`, `engine.rs::noise_point` ←
  `ckt/noisean.c`, `ckt/cktnoise.c`, `ni/niniter.c::NInzIter`,
  `dev/{res,bjt}noise.c` na forma ngspice): por frequência, o
  sistema AC ordinário dá o ganho entrada→saída e o sistema
  **adjunto** (transposto, `solve_complex_transposed_in_place` no
  backend) com corrente unitária na saída dá, num só solve, a
  transferência de cada gerador interno para a saída
  (`NevalSrc`: térmico `4kT·g`, shot `2q|I|`, flicker
  `kf·|I|^af/f`). Gerações por dispositivo via
  `Device::noise_suffixes/noise_dens`: R (térmico + 1/f + total,
  nomes ngspice `_thermal`/`_1overf`) e BJT (rc/rb/re térmicos com
  `gx` do estado, shot em Ic/Ib, flicker em Ib, total). Integração
  espectral por segmento log-log (`Nintegrate`, limiares
  `N_INTFTHRESH`/`N_INTUSELOG`), somatórios por fonte e por
  circuito, sumários a cada `pts_per_summary` pontos. A saída segue
  o formato **default** do ngspice: √(V²/Hz) (o `.options sqrnoise`
  de lá manteria os quadrados), plotnames curtos ("Noise Spectral
  Density Curves", "Integrated Noise"), vetores de densidade com
  nome cru e os integrados embrulhados `v(...)`/`i(...)` conforme a
  fonte de entrada é V ou I. Passo linear divide por n−1 (ngspice;
  o 3f5 usava n+1). Valida: `resnoise` e `bjtnoise` PASS (margens
  0.004/0.056); a diferença residual ~2e-5 vem das constantes
  físicas (o ngspice usa CODATA/SI-2019; o Dune mantém as do 3f5 —
  `corpus/KNOWN_DIFFS.md`).

Validação: `tests/op.rs` compara um deck-sonda (divisor, indutor,
diodo com RS, E, F, G, H, I) com números do ngspice-47 dentro das
tolerâncias SPICE; os decks `.op`/`.dc` de `corpus/basic/` passam no
oráculo contra o ngspice.

## Diferenças deliberadas

1. **Solver denso** (`linalg/backend/faer_dense.rs`): LU com pivoteio
   parcial do `faer` em matriz densa — o 3f5 usa o Sparse 1.3 com
   pivoteio de Markowitz. Para os circuitos do corpus é irrelevante
   numericamente; o backend esparso (faer sparse × perfil circuito do
   xingo-sparse) é a Fase 3 (P-3), atrás do trait `LinearSolver`
   (D-0005). `pivtol`/`pivrel` são aceitos e ignorados pelo backend
   denso. Singularidade: linha/coluna estruturalmente vazia é
   detectada antes do LU; a numérica aparece como solução não finita.
2. **`.nodeset`** usa sempre a forma de substituição de linha
   (`rhs = valor`, diagonal = 1); o C escolhe entre isso e uma
   condutância de 1e10 conforme a linha tenha contribuições de fontes
   de corrente (`ZeroNoncurRow`). Mesmo ponto fixo, caminho de
   convergência potencialmente diferente em decks patológicos.
3. **Erros estruturados** (`error.rs`) em vez de códigos `E_*` +
   impressões; as mensagens-aviso do C viram `notes` no resultado.
4. **`TMAX` default = `min(tstep, (tstop−tstart)/50)`** (ngspice,
   que restaurou o default do SPICE2; o 3f5 usa só `span/50`). É o
   que define a densidade de pontos do oráculo — sem isso o Dune
   dava passos ~15× maiores que o ngspice (ambos dentro do LTE) e as
   formas de onda divergiam na régua de comparação.
5. **UIC não emite o ponto t=0** (ngspice; o 3f5 despeja o vetor
   zerado intocado).
6. **NEWCONV**: além do teste de nós do `NIconvTest`, o 3f5 chama
   `CKTconvTest` → os testes de corrente por dispositivo
   (`dioconv.c`, `bjtconv.c`, `mos1conv.c`, `mos2conv.c`) a cada
   iteração candidata — o Dune implementa todos via
   `Device::conv_test` (uma versão anterior desta nota dizia,
   errado, que eles eram inertes sem bypass). São eles que forçam
   as junções a assentar na escala de µV, não de mV; sem eles o
   mosamp2 "convergia" com correntes frouxas.
7. **Fim de varredura `.dc` com epsilon** (ngspice): o 3f5 compara o
   valor acumulado cru com o stop e perde o último ponto quando o
   arredondamento passa de raspão; o ngspice (e o Dune) usam
   `> stop + DBL_EPSILON·1e3`. Passo zero/não-finito é erro (o 3f5
   entraria em laço infinito). Varreduras hostis têm teto de memória
   (~50 M valores) com erro limpo.

8. **Formas ngspice onde o 3f5 não fecha ou diverge do oráculo**
   (tabela completa em `corpus/KNOWN_DIFFS.md`): `DEVqmeyer` com
   `MAGIC_VDS` e a repartição gs/gd na inversão fraca; diodos de
   corpo do MOS em 3 regiões com gmin dentro da corrente; junções
   do BJT com gmin nos ramos de fuga e cauda polinomial abaixo de
   −3·n·vt; *dynamic gmin stepping* de Gillespie como default do
   `CKTop` (`gminsteps=1`; a escada do 3f5 vive em `gminsteps>1`);
   `srcFact` escalando toda fonte no TRANOP. Exceção na outra
   direção: o corte do MOS2 fica na fronteira `von` do 3f5 (o
   `vbin` do ngspice produz gm negativo com corrente nula e só
   converge com o dynamic gmin de lá; correntes idênticas).

## Pendências (fases seguintes)

- `.tf`, `.disto`; ruído dos demais dispositivos (diodo/MOS têm
  `*noise.c` no C, aqui ainda não; R sem fNcoef → flicker 0); `.pz` dos semicondutores restantes (BJT/
  MOS têm `pzLoad` no C, aqui ainda não); `.dc` varrendo TEMP e parâmetro de
  modelo; `keepopinfo` (plot "AC Operating Point").
- Semicondutores R/C com modelo (w/l), `temp` por instância de R.
- MOS nível 3, JFET/MESFET, chaves, linhas T/O/U, fonte B (Fase 3).
- Bypass de dispositivos (`CKTbypass`) — otimização, não semântica.
- Fallbacks restantes do op do ngspice: `gillespie_src` (rampa
  adaptativa de fontes) e o *transient op* (acha o `.op` por um
  pseudo-transiente — é o único que fecha um `.op` puro com BJT
  `off` atrás de fonte só-transiente; o caso `.tran` equivalente
  está coberto e regredido em `corpus/regress/offbjt.cir`).
