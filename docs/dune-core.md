# dune-core — notas de porte (Fase 1: `.op`)

Contrato: o C em `corpus/spice3f5/src/lib/{ckt,ni,dev,sparse}`.
API pública conforme D-0008 (specs tipados + `from_netlist` +
resultados espelhando o rawfile).

## O que foi portado fielmente

- **Constantes** (`consts.rs` ← `include/const.h`): `CHARGE`,
  `CONSTboltz`, `REFTEMP` etc., bit a bit.
- **Defaults de opções** (`options.rs` ← `cktinit.c`/`cktntask.c`):
  `reltol 1e-3`, `abstol 1e-12`, `vntol 1e-6`, `gmin 1e-12`,
  `itl1 100`, `temp/tnom 300.15 K`; `gminsteps`/`srcsteps` seguem
  o ngspice-47 (1 e 1, não os 10 do 3f5 — ver "Cadeia do op");
  `temp`/`tnom` chegam em °C no cartão.
- **Laço de Newton** (`engine.rs` ← `ni/niiter.c`): estados
  `INITJCT → INITFIX → INITFLOAT`, troca `rhs`/`rhsOld` por iteração,
  convergência exige `iterno != 1`, o passe extra `ipass` quando há
  `.nodeset`. Convergência (`ni/niconv.c`): `vntol` para tensões de
  nó, `abstol` para correntes de ramo, ambos com o termo `reltol`.
- **`CKTop`** (`ckt/cktop.c`): a cadeia completa do ngspice-47 com
  as escadas do 3f5 em `gminsteps/srcsteps > 1` — seção própria,
  "Cadeia do op", abaixo.
- **Stamps** dos lineares (`devices.rs` ← `dev/*/..load.c`): R, C
  (aberto em DC), L (fonte de 0 V com corrente de ramo), K (sem efeito
  DC), V (dc·srcFact), I, E, G, F, H — posições e sinais do C.
- **Diodo** (`diode.rs` ← `dioload.c`, `diotemp.c`, `devsup.c`):
  regiões exp/polinomial/ruptura com `gmin`, `DEVpnjlim` literal,
  limitação separada na ruptura, teste de convergência por corrente
  prevista (`cdhat`), nó interno `nome#internal` quando `RS≠0`,
  ajuste de temperatura de `IS` (EG/XTI) e o casamento iterativo de
  `BV`/`IBV` (25 iterações, mesmos avisos).
- **Gmin na diagonal** (`engine.rs::gmin_slots` ← `spsmp.c::LoadGmin`
  + `sputils.c::spMNA_Preorder`): o `diagGmin` vai para a diagonal
  *depois* do pré-ordenamento do Sparse — ver "Cadeia do op".
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

- **MOSFET nível 3** (`mos3.rs` ← `mos3load.c`, `mos3temp.c`,
  `mos3set.c`, `mos3cvtest.c`, `mos3acld.c`): o semi-empírico de
  canal curto (limiar com ETA/DELTA/largura estreita, THETA,
  VMAX/KAPPA, sublimiar NFS/xn), **nas formas alansfixes do
  ngspice-47**, que aqui vão além de derivadas: CLM suavizada sem
  VMAX (muda a corrente em até 3.5% perto de vdsat), derivadas
  corrigidas (o 3f5 tem `gds` de saturação com fórmula linear,
  `gmbs` incompleto e `gds0` lido sem inicializar), cauda cúbica
  nos diodos de corpo, `ni(TNOM)` no PHI de NSUB e os coeficientes
  de junção na forma MOS1/MOS2 (o `f4s` do 3f5 usa `tBulkPot` onde
  cabe `tDepCap`) — tudo em `corpus/KNOWN_DIFFS.md`, com gates
  medidos (reverter `gmbs`/`gds` FALHA mos3_inv a 9.6/317).
  `.options badmos3` é aceito e ignorado (sempre a forma default).
  Esqueleto compartilhado com MOS1/2 (MosGeom, DEVqmeyer, NEWCONV,
  17 estados); nós primos `#internal#drain`/`#internal#source`
  como no `mos3set.c` (e no MOS2). Extras do ngspice
  (`xl/wd/delvto/m/nlev`) e parâmetros sem case no `mos3mpar.c`
  (`XD/ALPHA/input_delta`) caem no aviso de parâmetro ignorado.
  Sondas: `regress/mos3_iv` (famílias NMOS/PMOS/quente/corpo
  polarizado, sublimiar→inversão forte) e `mos3_inv` (inversor
  CMOS 2 estágios + amplificador com degeneração para `gmbs` no
  `.ac`), PASS.

- **MOSFET nível 6** (`mos6.rs` ← `mos6load.c`, `mos6temp.c`,
  `mos6set.c`, `mos6trunc.c`; Sakurai–Newton): corrente de saturação
  `KC·(vgs−von)^NC`, tensão de saturação `KV·(vgs−von)^NV`, correção
  linear `(2−vdst)·vdst` abaixo de `vdsat`, limiar com
  `GAMMA·sqrt(φ−vbs) − GAMMA1·vbs − SIGMA·vds` (o 3f5 mata o termo
  SIGMA com um `;` perdido; o ngspice corrigiu e o Dune segue o
  ngspice — KNOWN_DIFFS), `LAMBDA0−LAMBDA1·vbs`, `KC` default
  `0.5·U0·COX`, alias `LAMBDA`→`LAMBDA0`, `vonbm` zerado onde o 3f5
  lê lixo (`sarg≤0`), `NVTH`/`PS` aceitos e ignorados como no 3f5.
  Sem carga AC nem pole-zero (o 3f5 não tem `mos6acld.c` nem
  `mos6pzld.c`; o ngspice também não — `.pz` com MOS6 é recusado).
  Mesmo andaime do MOS1.
  Valida: `mos6inv` (4 inversores NAND/NOR em subcircuitos, 44 MOS)
  PASS com máx 1.3 mV. Parâmetro de modelo desconhecido agora é
  aviso ("unrecognized parameter - ignored", como o `INPmakeMod`),
  não erro — os decks do 3f5 carregam XJ em cartões nível 1/6 que as
  tabelas desses níveis nem listam; e cartões de nível não suportado
  só viram erro quando algum M os instancia (mos6inv traz cartões
  nível 3 sem uso).

- **`.ac`** (`ac.rs`, `engine.rs::ac` ← `ckt/acan.c` — grade DEC
  na forma do **ngspice**: span > 1 década recalcula o passo para
  pousar exato em FSTOP (gate `regress/acdec.cir`; o 3f5 só
  coincide em décadas inteiras),
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
  `SMPcDProd`); matriz singular = raiz. Cargas `pzLoad` (`G + s·C`)
  para R C L K V I E F G H, diodo, chaves S/W e para todo
  semicondutor que tem `*pzld.c` no 3f5: BJT (`bjtpzld.c`), MOS1/2/3
  (`mos{1,2,3}pzld.c` — um arquivo renomeado, `mos1.rs::meyer_pz_load`),
  JFET/MES (`jfetpzld.c`/`mespzld.c`, `jfet.rs::fet3_pz`), BSIM1/2
  (`b1pzld.c`/`b2pzld.c`, `bsim1.rs::bsim_pz_load`; metade de
  condutâncias comum aos cinco MOS em `mos1.rs::mos_pz_conductances`)
  e BSIM3 (`b3pzld.c`). Todos são o `acLoad` com `s` no lugar de `jω`,
  lendo o mesmo estado do passe `INITSMSIG`. Desvios do arquivo C:
  BJT e JFET estampam a resistência série **como condutância × área**
  (o ngspice; o 3f5 estampa `R·area`, ohms como siemens —
  KNOWN_DIFFS, gates 497×/500×); a fase em excesso do BJT fica fora
  (`xgm = 0` nos dois C). Quirk literal: o `CKTpzSetup` re-roda os
  setups e o `mos3set.c` zera `MOS3mode`, então o MOS3 entra no `.pz`
  sempre em modo normal (gate 995×; KNOWN_DIFFS). O MOS6 não tem
  `pzLoad` em nenhum dos dois C (nem `acLoad`): o C o pula em silêncio
  e o determinante sai sem ele; o Dune recusa o deck (dispositivo sem
  porte falha alto). Sondas `regress/pz_{bjt,mos1,mos2,mos3,jfet,mes,
  bsim1,bsim2,sw}` — CE/CS com resistência de fonte, seguidor com
  pinos de fonte/dreno trocados nos MOS (modo reverso), RD/RS/RC/RE
  não nulos —, todas PASS contra o ngspice-47 (margens ≤ 0.07);
  verificação extra-oráculo: os polos/zeros de cada sonda,
  reconstruídos em `K·Π(s−z)/Π(s−p)`, reproduzem o `.ac` do ngspice
  (V(out)/V(in), 1 kHz–1 THz) a ≤ 4e-4 (o BJT; os MOS/FET a ≤ 4e-7) —
  exceto `pz_mos3`, onde o quirk do modo afasta os dois simuladores
  do próprio `.ac` pelo mesmo 1.2 relativo. Bugs do C tratados: o
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
  nomes ngspice `_thermal`/`_1overf`), BJT (rc/rb/re térmicos com
  `gx` do estado, shot em Ic/Ib, flicker em Ib, total), diodo (7
  vetores do ngspice — os 3 `*sw` sempre nomeados, zero sem RSW),
  MOS1/2/3 (`mos*noi.c` do ngspice-47 com os seletores `NLEV`/
  `GDSNOI`, parâmetros só do ngspice, default NLEV=2: canal
  `(2/3)|gm|` para NLEV<3 e `(2/3)·β·vgst·(1+α+α²)/(1+α)·GDSNOI` para
  NLEV=3 — α=0 na saturação, `1−vds/vdsat` na região linear, zero
  abaixo do limiar —; flicker NLEV=0 `KF·|Id|^AF/(f·Leff²·Cox)`, 1
  `KF·|Id|^AF/(f·W·Leff·Cox)`, 2/3 `KF·gm²/(f^AF·W·Leff·Cox)`; NLEV
  fora de 0..3 cai fora do `switch` do C e deixa o ganho cru, 1 A²/Hz
  — mantido; o 3f5 usava Id^AF/Cox², que no ngspice só volta com
  `ngbehavior=spice3`, não modelado; fallback tox=1e-7 sem TOX no
  MOS1), JFET/MESFET (`fet3_noise` compartilhado; o JFET com o
  `NLEV=3`/`GDSNOI` do `jfetnoi.c`, `α = 1 − vds/vgst` com os vgs/vds
  crus do state0; o MESFET sem seletor), BSIM1/BSIM2 (`b1noi.c`/
  `b2noi.c` do ngspice-47 — o 3f5 não tem ruído deles nem `KF`/`AF`:
  rd/rs, canal `(2/3)|gm|`, flicker `KF·|Id|^AF/(f·(W−DW)·(L−DL)·
  Cox²)` com Cox em F/cm²; ver a divergência abaixo) e chave S/W (um
  gerador térmico, integrado como no `swnoise.c` — dispositivo de
  gerador único integra o próprio total); MOS6 não tem em nenhum dos
  dois. **Temperatura dos térmicos** (`NevalSrcInstanceTemp` do
  ngspice-47): R, BJT (rc/rb/re), diodo (rs), JFET (rd/rs/id) e
  MOS1/2/3 (rd/rs/id) usam a temperatura da instância — `TEMP=`
  quando o cartão o dá, senão `CKTtemp + DTEMP` (`*Temp::temp`,
  `NoiseCtx::thermal_at`); MESFET, chaves, BSIM1/2 e BSIM3 ficam em
  `CKTtemp` (`NevalSrc` puro), como no C; shot e flicker não
  dependem de T. O R segue o `resnoise.c` do ngspice-47 (`dtemp =
  REStemp − CKTtemp` com `TEMP=`, senão `RESdtemp`; condutância
  `RESconduct`, já com o fator de TC) — o 3f5 usava `CKTtemp`
  (KNOWN_DIFFS; `regress/rnoisetemp` PASS 0.004, com `CKTtemp` no R
  FALHA a 111.9).
  **BSIM1/2 contra o oráculo:** o `bsim1def.h` define `B1gm`/`B1cd`
  como *offsets* de estado (`#define B1gm B1states+ 7`), e o
  `b1noi.c` lê `fabs(inst->B1gm * inst->B1m)` = `GENstate + 7` e
  `(double)inst->B1cd` = `GENstate + 4` — o índice da instância no
  vetor de estados, não gm e Id (medido: um BSIM1 sozinho no deck dá
  o ruído de canal de 7 S). O Dune lê os valores (`state0`), como o
  autor queria e como o `jfetnoi.c` faz; `regress/b1noise`/`b2noise`
  ficam no `corpus/SKIP` e o teste `bsim1.rs::noise_tests` reproduz
  os goldens do ngspice-47 vetor a vetor (≤1.24e-5) emulando a
  leitura do offset (`GENstate = 0`, um BSIM por deck, sem BJT —
  na tabela de dispositivos do ngspice só URC/ASRC/BJT/BSIM1 vêm
  antes) e confirma rd/rs iguais sem a emulação. Integração
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

- **`.disto`** (`disto.rs`, `engine.rs::disto` ← `ckt/distoan.c`,
  `ckt/cktdisto.c`, `ckt/dkerproc.c`, `ckt/dloadfns.c`,
  `ni/niditer.c`; Roychowdhury 1988): método de Volterra em torno
  do ponto de operação. H1(f1)/H1(f2) são solves AC ordinários
  excitados pelos `DISTOF1`/`DISTOF2` das fontes (o `AC` delas não
  participa); cada kernel de ordem superior é mais um solve
  complexo do mesmo sistema linearizado, na frequência de
  combinação, com RHS vindo das não-linearidades
  (`Device::disto_setup/disto_load`: coeficientes de Taylor no OP
  contraídos com os kernels menores pelas funções multinomiais
  `S*`/`D1*` do `dloadfns.c`, portadas uma a uma com testes
  unitários contra produtos complexos). `DkerProc` escala 2/2/4/4/6
  para amplitudes de senóide. Sem `F2OVERF1`: plots do 2º e 3º
  harmônicos; com: IM f1+f2, f1−f2, 2f1−f2, com ω2 fixado **uma
  vez** em `2π·FSTART·F2OVERF1` (o comentário hbsim do C). LIN usa
  `delta=(stop−start)/(N+1)`, diferente do `.ac`. Dispositivos:
  diodo (`diodset.c`/`diodisto.c`: junção, difusão e depleção, com
  o VJ *do modelo*, não o ajustado por temperatura, e o ramo de
  ruptura sobre `vt` — tudo como o C, dúvidas dele preservadas em
  comentário); os lineares declaram-se sem não-linearidade
  (`DEVdisto == NULL`); BJT/MOS/BSIM ainda não (erro estruturado).
  Fontes estampam por **atribuição** (não soma), VSRC no ramo, ISRC
  nos nós com a polaridade do ngspice (flip de ordem ímpar medido —
  `corpus/KNOWN_DIFFS.md`). **Dispositivos** (2026-11-06): diodo,
  BJT NPN/PNP (`bjtdset.c`/`bjtdisto.c` — rotação de fase em
  excesso via `omega1/omega2` no `DistoCtx`, termos cruzados
  vbe/vbc pelos `DFn*/DFi*` multivariáveis) e MOS1/2/3
  (`mos*dset.c`/`mos*dist.c` — um arquivo com prefixo trocado;
  partes comuns em `mosdisto.rs`). A álgebra de derivadas
  (`Dderivs`, as 13 operações de `dev/disto/*.c`) vive em
  `dderivs.rs`, cada regra testada contra diferenças finitas —
  as 13 fórmulas do C são exatas. Quirks do C mantidos (e
  compartilhados pelo ngspice): o `fgate` extra do MOS3
  (`mos3dset.c:518`, pego porque o mos3disto falhava 4% sem ele),
  as leituras indefinidas do MOS2 dSetup lidas como zero
  (medido), vbc do nó externo no BJT, kernels vbb/vsc não
  conjugados no F1MF2, `ufact=0` abaixo do campo crítico no MOS2
  — detalhes no KNOWN_DIFFS. **JFET/MESFET** (2026-11-08,
  `jfet.rs::jfet_dsetup`/`mes.rs::mes_dsetup` + `fet3_disto_load` ←
  `jfetdset.c`/`jfetdist.c`, `mesdset.c`/`mesdisto.c`; o ngspice-47
  só acrescenta `m` e beta/limiar ajustados por temperatura, que o
  JFET do Dune não tem — idênticos nos defaults): corrente de dreno
  em x=vgs, y=vds (o JFET em fórmulas fechadas por região, o MESFET
  pela cadeia `Dderivs` do C), os dois diodos de porta (`vgd = x−y`)
  e as cargas de porta — no JFET duas depleções de uma variável, no
  MESFET duas cargas em (vgs, vgd). Quirks literais (os dois
  simuladores têm o mesmo código): o setup do JFET é
  Shichman–Hodges puro (ignora o `B` de Sydney que o load usa); no
  modo inverso ele troca vgs/vgd *antes* das junções mas os nomes
  locais guardam czgs/czgd, e o bloco final reescreve as 2ªs ordens
  dos locais — o ramo gs fica com a 2ª ordem do diodo gd, a carga de
  czgs no vgd real (2ª) e a de czgd no vgs real (3ª); o MESFET
  troca o `qggnew` do load por `qgs = vgs·cgsnew` ("probably wrong",
  diz o comentário) com `czgs/(qroot·par1)` onde o `qggnew` tem
  `czgs/qroot·par1`. **BSIM1** (2026-11-08, `b1disto.rs` ←
  `b1dset.c`/`b1disto.c`, idênticos no 3f5 e no ngspice-47 fora o
  `M` que o Dune não tem): o setup é uma transcrição própria do
  `B1evaluate` com `Dderivs` (p=vgs, q=vbs, r=vds do dispositivo
  normalizado), corrente de dreno e cargas qg/qb/qd nas duas
  partições, junções de corpo, regra da cadeia do modo inverso e
  fatores de Taylor; o load contrai com os kernels (dreno, três
  cargas com jω, dois diodos). Não é o `B1evaluate` derivado:
  quirks portados literalmente — Beta0 quadrático multiplicado por
  `d_dummy` (1+Ugs·VgsVth) em vez de vds (`b1dset.c:397`), `−Eta·vds`
  derivado com Eta constante (`:307`), sublimiar só na janela
  −40·N0·vt < VgsVth < 20·N0·vt, `co4v15 = 1/15` na carga de dreno
  40/60 saturada (`:744`; o load usa 4/15), `capbd2` direto lido das
  áreas de fonte (`:991`) e o termo `gbd` estampado a partir do
  **gate** em TWOF1/F1PF2/F1MF2 (`b1disto.c:284/995/1307`).
  **MOS6 e BSIM2 não têm `DEVdisto`** nem no 3f5 nem no ngspice-47
  (sem `*dset.c`; conferido) e seguem com o erro estruturado.
  Derivadas vs diferenças finitas: JFET 4.4e-8 relativo (polinomial,
  arredondamento), MESFET 2.5e-6 (corrente) e 2.7e-6 (cargas, com
  Richardson), BSIM1 ≤8e-5 (truncamento, h=5e-4); valores do setup
  do BSIM1 = `B1evaluate` a 1e-12 fora dos quirks. Sondas novas:
  `jfetdisto` (harmônicos; NJF saturado, NJF em modo inverso com as
  duas junções dentro de −5·vt, PJF com porta além de FC·PB; PASS
  0.168/0.148), `mesdisto` (IM; NMF saturado, NMF com porta direta e
  `vnew1` grampeado, NMF inverso, PMF linear; PASS 0.301/0.046/
  0.077), `b1disto` (harmônicos, 40/60, degeneração de fonte; PASS
  0.000) e `b1disto_im` (modo inverso, XPART=1, junção dreno-corpo
  direta, IM; PASS 0.084/0.084/0.007). Sondas: `mixdisto_hd`/`mixdisto_im`
  (derivadas do mixdisto do corpus, antes em SKIP), `bjtdisto2`
  (NPN IM, fontes Norton — cruzamento de zero do Im de corrente
  não tem régua), `bjtdisto3` (PNP harmônicos, CJS=0 pela
  divergência de substrato), `mosdisto`, `mos2disto`,
  `mos3disto` (PMOS IM). Valida: `diodisto` PASS nos 3 plots IM;
  o resíduo de 1.7e-4/3.7e-4 relativo contra o golden é o Newton do
  ngspice parando ~3e-6 V antes do exato + as constantes físicas —
  com tolerâncias apertadas e as constantes do ngspice, o porte
  bate a ≤3e-13 em quatro decks de sonda (harmônicos, IM com fases,
  diodo com RS e reverso, excitação por ISRC).

- **`.tf`** (`tf.rs` ← `ckt/tfanal.c`, idêntico nos dois): dois
  solves extras contra o Jacobiano fatorado do op — excitação
  unitária no ramo da fonte de entrada dá ganho e resistência de
  entrada; corrente unitária no nó de saída dá a de saída. Clamps
  do C mantidos (1e20 quando a corrente do ramo < 1e-20;
  `1/max(1e-20, i)` para saída em corrente). Plot `Transfer
  Function`, vetores `v(Transfer_function)`,
  `v(<fonte>#Input_impedance)`, `v(output_impedance_at_V(n,m))`.
  Sonda: `regress/tf_amp` (BJT + E/G/F/H, saída em par de nós).

- **`.sens`** (`sens.rs` ← `ckt/cktsens.c`/`cktsgen.c` — a
  perturbação `sens_sens` do próprio 3f5; o adjunto de Meikle
  nunca foi o que roda, nem lá nem no ngspice): cada parâmetro
  move 1e-6·valor (1e-6 se zero), o dispositivo recarrega numa
  matriz vazia e resolve-se `Y⁻¹(ΔI − ΔY·E)/δ` contra a matriz
  fatorada. Extensões do ngspice adotadas: parâmetros zero
  listados, filtros glob no cartão, plot `Sensitivity Analysis`
  com vetores `v(<inst>:<kw>)`/`v(<inst>_<kw>)`. Cobertura:
  R/C/L/K/V/I/E/F/G/H + diodo (tabela do 3f5); outros dispositivos
  → erro estruturado. Quirks dos dois C mantidos (LIN multiplica o
  passo — `LINEAR` vs `SENS_LINEAR` —, OCT multiplica por ln2 onde
  devia dividir; `rs` do diodo sem efeito, condutância fixada no
  setup). Parâmetros de temperatura do R (contrato medido no C,
  2026-11-08): o `cktsens.c` do 3f5 chama `sens_setp` (`:399`) e
  depois `sens_temp` (`:420`, `DEVtemperature` = `REStemp`) antes de
  recarregar, e o `REStemp` refaz `RESconduct` com o `tc1`/`tc2` do
  modelo (`resmpar.c`, `IOPQ`/`IOPQO`) ou o `temp` da instância
  (`resparam.c`, `IOPZU`) movidos — os três são **vivos** no 3f5 e
  no Dune (`ResistorI::sens_set`; `d/dTC1 = ∂V/∂R·R·ΔT`, `d/dTC2 =
  ∂V/∂R·R·ΔT²`, `d/dTEMP = ∂V/∂R·R·(TC1 + 2·TC2·ΔT)`); `_temp` move
  a temperatura efetiva (com `DTEMP`, o ngspice salta −DTEMP);
  `_dtemp` inerte (o ngspice dá 0 na varredura completa); `:tce`
  (só-ngspice) inerte — no ngspice ele liga `REStceGiven` para
  sempre e estraga as perturbações seguintes do modelo, daí os
  filtros de `regress/sens_rtemp` (PASS a ≤5e-7 da régua; inertes
  FALHA a 500). **Diverge do oráculo onde o ngspice está quebrado** (AC
  com não-lineares zerados, K e G/F errados — verificado por
  diferença finita de dois `.ac` do próprio ngspice; 6 entradas no
  KNOWN_DIFFS). Sondas: `sens_dc` (divisor+diodo), `sens_ac` (1
  frequência, lineares), `sens_acsweep` (LIN com o passo
  multiplicado pinado).

- **`.sp`** (`sp.rs`, `engine::sp_prepare/sp_point` ← `span.c` do
  **ngspice-47**, contrato E oráculo — pós-3f5; Fase 3.5): portas =
  VSRCs com `portnum`/`z0` (default 50), que no ngspice mudam a
  fonte em TODAS as análises (nó `<nome>#res` + condutância 1/Z0,
  `vsrcset.c:56` — o `.op` enxerga o DC através de 50 Ω; `.pz`
  carrega só o ramo). Por frequência: um load AC em modo SP (AC de
  VSRC desligado, ISRC fica), uma fatoração, um solve por porta com
  1 V no ramo; ondas de potência `a,b=(V±Z0·I)/(2√Z0)`, `S=B·A⁻¹`,
  Y/Z pelas fórmulas do span.c (inversão por Gauss–Jordan com
  pivoteio — o ngspice usa adjugada, custo fatorial); vetores
  `S/Y/Z_i_j`, e com 2 portas NF/SOpt/NFmin/Rn de `Cy/(4kT)`.
  `donoise=1`: adjunto por porta, `Cy += i·iᴴ` (flicker descartado
  — caminho gain-only do NevalSrc, medido). Passo DEC fica no
  `10^(1/n)` (o `.sp` de lá não re-espaça no FSTOP como o `.ac` —
  pinado por sp_amp em span não-inteiro). Quirks literais: bug de
  memcpy do span.c:779 (base imaginária vira a real; só com ISRC AC
  de fase), ordem dos cartões no reset de z0 (`z0=-1 portnum 1` é
  porta; invertido não é), checks do VSRCtemp (portas 0/1, ordem,
  duplicata). `pwr/freq/phase` (fonte PORT) recusados no parser.
  Sondas: `sp_2port` (RLC, Z0 50/75, ~1e-15 apertado), `sp_amp`
  (MOS1, não-recíproco S21≠S12, 3e-13), `sp_noise` (BJT donoise;
  NF/Rn/SOpt/NFmin a 1e-6–1e-5). Unit: T-pad casado K=2 fechado
  (S21=1/K, Bosma `Cy=4kT·Re(Y)`, NF=20·logK).

- **LTRA** (`ltra.rs` ← `ltraset.c`, `ltratemp.c`, `ltraload.c`,
  `ltraacct.c`, `ltratrun.c`, `ltraacld.c`, `ltramisc.c`;
  Roychowdhury): linha com perdas por convolução. As duas equações
  de porta são incógnitas *de tensão* (`CKTmkVolt`), nomeadas
  `<nome>#i1/#i2` e salvas no rawfile como o ngspice
  (`v(o1#i1)` — o filtro `saved_node()` agora esconde só os seis
  sufixos internos do ngspice, não todo `#`). Quatro regimes:
  LC (atraso puro), RLC (respostas ao impulso h1'/h2/h3' com os
  fits de Bessel I0/I1 do Numerical Recipes), RC (kernels erfc,
  `libm`) e RG (atenuador sem frequência). Coeficientes =
  segundas diferenças das respostas duas-vezes-integradas na malha
  de pontos aceitos (lista própria, o `CKTtimePoints` do C),
  recomputados na 1ª iteração de cada ponto; valores atrasados por
  interpolação quadrática/linear. Breakpoints a múltiplos do
  atraso via `accept`; teto de passo (`steplimit`/RLC safe step)
  via `trunc` (`TerrCtx` ganhou `sol`/`time`); "cannot find
  delayed timepoint" vira `LoadCtx::fault` → não-convergência →
  corte de passo (o `E_INTERN` do C). `.options trytocompact`
  portado (seção "trytocompact" abaixo). `LTRAlteCalculate` fica de
  fora por ser inalcançável: só roda com `LTRAlteConType` ≠
  `NOCONTROL` (`ltratrun.c:129`), e `ltraset.c:75-77` força
  `NOCONTROL` a menos que `fullcontrol`/`halfcontrol` tenham sido
  dados — flags que estão sob `#ifdef notdef` na tabela do 3f5
  (`ltra.c:61-73`) e nem existem na do ngspice-47 (só `nocontrol`).
  O resto do `LTRAtrunc` (teto `steplimit`, o teste de inclinação sob
  `nosteplimit`, o `LTRAmaxSafeStep` do RLC) está no `trunc`. Exigiu
  três acertos fora do LTRA (todos em `corpus/KNOWN_DIFFS.md`):
  a **regra de aproximação de breakpoint** do ngspice (resto
  < 1.9·passo → dois passos iguais; medida na fronteira
  1.899–1.901), o **`pnjlim` reverso** do ngspice e a **limitação
  do predictor do diodo** (bug de fidelidade; `dioload.c` limita a
  extrapolação — gate: ltra_1, que FALHA 1.061 sem ela). Valida:
  ltra_1 PASS 0.058, ltra_2 PASS 0.003 (RLC concentrado — gate da
  regra de breakpoint: 1.588 sem ela), `corpus/regress/ltra_rc.cir`
  e `ltra_rg.cir` em lockstep (3e-14). **ltra_3 é XFAIL (teto
  35)**: ringing de trapézio na corrente dos diodos do `dioload`
  (rs + carga de difusão, sem cjo) — o golden do ngspice ringa
  período-2 por intervalos inteiros, o Dune perde o ring em 8.49 ns
  e o reinício em ordem 1 o amortece; o próprio ngspice re-rodado
  a TMAX=0.01 ns difere do próprio golden por 26.9×; sem os
  vetores x3.* o deck passa a 0.42, e a linha em si bate a 1.2 mV.
  Mesma família do schmitt/mosamp2 (D-0011/D-0012). Nota menor:
  nó x1.6 (flutuante, definido por fugas) fica 7 mV do ngspice no
  op — não afeta vereditos.

- **BSIM3v3.3.0 (níveis 8/49)** (`bsim3/` ← `b3set.c`, `b3temp.c`,
  `b3ld.c`, `b3cvtest.c`, `b3acld.c`, `b3pzld.c`, `b3noi.c` do
  **ngspice-47**, que aqui é contrato E oráculo — o modelo não
  existe no 3f5; Fase 3.5 por decisão do usuário): o b3ld.c
  inteiro fora NQS e bypass — 3 mobMods, capMod 0–3 com as 3
  partições XPART, os 3 modelos de overlap, junções com limite
  ijth; parâmetros em knots por (L,W) cacheados como o C
  (binning L/W/P, BINUNIT 0/1); versão default "3.3.0", outras
  versões/NQS/ACM≠0 recusados com erro adiado à instanciação.
  Ruído noiMod 1–6 com vetores `onoise.m1.rd` como o ngspice
  (gancho interno `noise_sep`). API: `Bsim3Model` com acesso por
  keyword (`set/get` — 413 parâmetros reais + 9 seletores; campos
  públicos seriam ~420) e spec `Bsim3` com `m`/`delvto`/`mulu0`
  da tabela de instância do ngspice. Quirk literal: capMod 3 sem
  o `−T3` em `dVdseffCV_dVb` (`b3ld.c:2119`; BSIM4 corrigiu).
  Sondas: `bsim3_{iv,vg,inv,cap,noise,pz}` com os cartões 0.35 µm
  de `examples/mos/modelcard.*` do ngspice — todas PASS (pior
  margem 0.211, piso de abstol em sublimiar profundo); gates
  medidos no ruído (PMOS→noiMod 1 falha a 17; KF=0 a 476).
  Resta do BSIM3: NQS, ACM, `.disto`/`.sens`.

- **Chaves S/W** (`switch.rs` ← `swload.c`/`cswload.c` do
  **ngspice-47** — o 3f5 tem outra máquina, ver KNOWN_DIFFS):
  um tipo só, controlado por par de nós (S) ou pela corrente de
  uma Vsource (W, resolvida como F/H). Máquina de 4 estados
  (`REALLY_ON/OFF` fora da janela, `HYST_ON/OFF` dentro), ramo
  separado para histerese ≤0, hints `ON`/`OFF` na escada de
  inicialização, flip de estado marca não-convergência; `SWtrunc`
  limita o passo ao se aproximar do limiar; AC/pz/ruído leem
  state1 (um gerador térmico, `onoise_s1`). GOFF default = gmin;
  RON/ROFF zero ou não-finito é erro estruturado na elaboração
  (o C faria 1/0). O `.dc` do Dune espelha a rotação de bancos do
  `DCtrCurv` com um `copy01` por ponto (só as chaves leem state1
  em DC). Sondas: `regress/sw_hyst` (janela varrida nos dois
  sentidos via nó espelhado, VH negativo, defaults) e `csw_rly`
  (relé por corrente, IH negativo), PASS a ~1e-14. Nota de deck:
  o ngspice agrupa análises por tipo e roda cada tipo em ordem
  **reversa** do deck — um deck novo com duas análises do mesmo
  tipo não pareia no oráculo; use nós espelhados.

- **Linha de transmissão ideal T** (`tra.rs` ← `traload.c`,
  `trasetup.c`, `traacld.c`, `tratrunc.c`, `traacct.c`): método das
  características — dois ramos Thevenin cujas fontes leem tensão e
  corrente da OUTRA porta atrasadas de TD, com tabela de histórico
  própria e interpolação; DC = Z0 entre as portas com o quirk
  `(1−gmin)·Z0` do C; UIC carrega por `IC=`/`V1..I2`; atraso por
  `TD` ou `F`/`NL` (defaults F=1GHz, NL=0.25); REL/ABS são
  só-saída na tabela do C (não parseiam). Incógnitas extras
  `#i1/#i2/#int1/#int2` (ordem do C, tipadas tensão, todas salvas
  como no rawfile do ngspice). Exigiu três acertos no laço
  transiente (KNOWN_DIFFS): o "take earlier point" do
  `cktsetbk.c:37-40` que faltava (fidelidade), o teste de
  breakpoint estrito (`>`) do ngspice e o breakpoint em TSTEP sob
  UIC — baseline medida: só tra_pulse e truic mudam no corpus
  (truic aperta de 1.8e-3 a 7.5e-15). `.pz` com linha T: recusa
  estruturada (o ngspice a ignora em silêncio — sem pzLoad lá).
  Sondas em lockstep (≤6e-8): `tra_pulse` (casada + descasada com
  reflexões, 136 breakpoints plantados), `tra_ac` (stubs aberto e
  curto), `tra_uic`.

- **JFET e MESFET** (`jfet.rs` ← `jfetload.c`, `jfettemp.c`,
  `jfetset.c`, `jfetacld.c`, `jfettrun.c`; `mes.rs` ← `mesload.c`,
  `messetup.c`, `mestemp.c`, `mesacl.c`, `mestrunc.c`): o
  `mesload.c` é o `jfetload.c` linha a linha fora corrente, carga e
  `CONSTvt0`, então o esqueleto comum dos FETs de 3 terminais vive
  em `jfet.rs` e os dois usam (`Fet3Nodes`, `fet3_bias` — escada de
  inicialização, predictor, `pnjlim`+`fetlim` —, `gate_diode` com
  gmin, `fet3_charge`, `fet3_finish` — teste de convergência por
  concordância de correntes *dentro do load*, como no C, que não
  tem `*cvtest.c` para eles —, `fet3_ac`). JFET: Shichman–Hodges
  com o rabo de dopagem `B` de Sydney (`Bfac=(1−B)/(PB−VTO)`),
  `JFETtemp` completo (os parâmetros térmicos extras do ngspice —
  XTI/EG/VTOTC… — são ignorados com aviso; nos defaults o ngspice
  reproduz o 3f5, medido a 2e-4 a 80 °C e pinado por `j3` em
  `jfet_iv`). MESFET: Statz com saturação contínua em `3/ALPHA`;
  default de BETA é **2.5e-3** (`messetup.c` e ngspice; o
  spice.txt documenta 1e-4 errado). Nós primos `#source`/`#drain`
  (source primeiro, ordem do C). `LEVEL≠1` (Parker–Skellern, MESA,
  HFET do ngspice) difere como os níveis MOS: só erra se
  instanciado. Ruído via `fet3_noise` (pinado por `regress/fetnoise`,
  `nlevnoise`, `noisetemp`); `.disto` pelo `jfetdset.c`/`mesdset.c`
  (`regress/jfetdisto`, `mesdisto`); `.pz` via `fet3_pz`
  (`regress/pz_jfet`, `regress/pz_mes`). Sondas: `regress/jfet_iv` (famílias I-V NJF/PJF/quente,
  margem 0.27 — constantes físicas ×v/vt no diodo de porta),
  `jfet_amp` (op/ac/tran), `mes_iv`, `mes_amp` (carga de Statz com
  porta direta além de `vmax`), todas PASS.

- **BSIM1 (nível 4)** (`bsim1.rs` ← `b1ld.c`, `b1eval.c`,
  `b1temp.c`, `b1set.c`, `b1moscap.c`, `b1acld.c`, `b1cvtest.c`):
  cada parâmetro elétrico vem em tripla (valor, dep. L, dep. W)
  resolvida por instância como `p0 + pL/Leff + pW/Weff` com
  Leff/Weff em µm; `TOX`/`DL`/`DW` em µm e mobilidades em cm², com
  a dança de unidades do C intacta (`Cox = 3.453e-13/(tox·1e-4)`
  F/cm²). Corrente: interpolação quadrática de Beta0 entre VDS=0 e
  VDD, mobilidade `1/(1+UGS·(vgs−vth))`, saturação pelo `K` de
  `0.5(1+Vc+√(1+2Vc))`, sublimiar exponencial com `Ilimit` (fix de
  Peter M. Lee de 1990 mantido), tudo sobre `CONSTvt0` (BSIM1
  ignora a temperatura do dispositivo). Cargas analíticas por
  região (acumulação/sublimiar/trioda/saturação) nas partições
  0/100 (`XPART≥1`) e 40/60, com as condutâncias-companheiras do
  `B1mosCap` (`ag0`·matriz de capacitâncias) e o swap d↔s no modo
  inverso. 35 estados, 22 posições, `B1convTest` em
  `Device::conv_test`, LTE em qb/qg/qd. O ngspice carrega este
  código sem mudanças. Valida: `bsim1tst` (varredura `.dc` de 5
  transistores) PASS a 2.8e-10 A. **Armadilha dos overlaps:** o
  `b1ld.c` monta os caps de overlap do transiente com W *desenhado*
  (gs/gd) e L *efetivo* (gb), enquanto o `b1temp.c` guarda na
  instância os valores com W efetivo e L desenhado — e só o
  `b1acld.c` usa estes. São dois conjuntos (`ld_*` vs os do temp);
  usar o do temp no load erra as cargas quando DL/DW ≠ 0
  (regressão: `corpus/regress/b1overlap.cir`, acoplamento por Cgdo
  num nó de 1 fF — 0.118 V de erro antes da correção).

- **BSIM2 (nível 5)** (`bsim2.rs` ← `b2ld.c`, `b2eval.c`,
  `b2temp.c`, `b2set.c`, `b2cvtest.c`, `b2acld.c`): triplas
  valor/L/W como no BSIM1 (`p0 + pL·(1e-6/Leff) + pW·(1e-6/Weff)`),
  com os clamps do `b2temp.c` (`betas0 ≥ 1.01·beta0`, teto de
  `betasB`) e `Arg = betasB − beta0B − vdd·(beta3B − vdd·beta4B)`.
  O `B2evaluate` inteiro: limiar com `Gg`/`Aa`, spline cúbica de
  `Vgeff` entre VGLOW e VGHIGH (sublimiar exponencial abaixo),
  mobilidade vertical `Uvert`, saturação por `Vc`/`Kk`, interpolação
  tanh de Beta em VDS, ionização por impacto `AI`/`BI` (fator FR), e
  o modelo de cargas por região com o recálculo de Vdsat *sem* a
  suavização de Vgeff dentro do bloco de cargas (quando
  `Vgst ≥ VGHIGH` as derivadas de Vdsat da seção DC são
  reaproveitadas, como no C; o `*vdsat` reportado é o recalculado).
  Defaults não-nulos do `b2set.c` (VFB=−1, PHI=0.75, K1=0.8,
  MU0=400, N0=1.4, VOF0=1.8, VGHIGH=0.2, VGLOW=−0.15, TOX=0.03 µm,
  VDD=VGG=VBB=5, WDF=10…); L/W de instância caem em 5 µm, não nos
  defaults de `.options`. O shell de load/conv/AC/trunc é o do
  BSIM1 (o C é idêntico fora a indireção `pParam`), overlaps na
  mesma convenção dupla do BSIM1, junções de corpo sobre
  `CONSTvt0`. Os modos de diagnóstico do `valuetypeflag` (XPART
  negativo) não vieram. O ngspice carrega este código sem
  mudanças. Valida: `bsim2tst` (`.dc`) PASS a 8.9e-16; sondas
  extra-oráculo tran (inversor CMOS com RSH) e `.ac` contra o
  ngspice-47 a 1.2e-2 (tubo, shift 0) e 1.6e-13.

  **Caso-limite conhecido:** nó quase-flutuante (1 fF) cujo `vds`
  cruza 0 exatamente num breakpoint mata o passo ("timestep too
  small") no BSIM1/2 — o flip de modo d↔s no canto; o ngspice
  sobrevive por margem. Não gateia nenhum deck do corpus (o
  `b1overlap.cir` polariza o nó para evitar o cruzamento); fica
  para a investigação do controlador de passo.

- **Linha U (URC)** (`elaborate.rs::urc_expand` ← `urcsetup.c`):
  no 3f5 a URC não tem load — o setup a **expande** numa escada de
  elementos concentrados, e é isso que o Dune faz na elaboração.
  Modelo `URC` (`urcmpar.c`: `K`=1.5, `FMAX`=1 GHz, `RPERL`=1000,
  `CPERL`=1e-12, `ISPERL`, `RSPERL`=0, flag `URC` inerte), instância
  `L` e `N`. Lumps: `N` truncado se dado, senão
  `max(3, ln(wnorm·((p−1)/p)²)/ln p)` truncado, com
  `wnorm = FMAX·R0·C0·2π` e 3 quando `wnorm < 35`. Seções:
  `r1 = R0(p−1)/(2pⁿ−2)`, `c1 = C0(p−1)/(pⁿ⁻¹(p+1)−2)` (idem `i1`
  para IS), crescendo por `p` a partir **das duas pontas** (`rlo<i>`
  do lado de N1, `rhi<i>` do lado de N2) até se encontrarem no último
  nó `hi`; cada nó interno ganha `clo<i>`/`chi<i>` para N3 — ou
  diodos `dlo<i>`/`dhi<i>` de área `p^(i−1)` com `CJO=c1`, `IS=i1`,
  `RS=L·n·RSPERL` quando `ISPERL` é dado. Nomes do
  `IFnewUid`/`CKTmkVolt` (`u1#hi1`, `u1#lo1`, `u1#rlo1`…): o ngspice
  salva esses nós no rawfile e o Dune também (`saved_node` não os
  esconde; o `#internal` dos diodos com RS sim). O `urcsetup.c` do
  ngspice-47 é o do 3f5 (só cast/estilo), então não há
  KNOWN_DIFFS de modelo. Guardas novas (o C divide por zero ou aloca
  sem limite): `L ≤ 0`, `K ≤ 1` e mais de 10 000 lumps são erro.
  Sondas: `urc_line` (`.ac` + `.tran`, 10 lumps pela regra e N=4) em
  lockstep (3.5e-14 no tran, 9.5e-7 no AC) e `urc_diode` (`.dc` +
  `.tran` com ISPERL) a 0.52/0.017.

- **Fonte B (ASRC)** (`asrc.rs` ← `asrcload.c`, `asrcacld.c`,
  `asrcpzld.c`, `asrcconv.c`, `asrcset.c`, `inpptree.c`, `ifeval.c`,
  `ptfuncs.c`; parser em `dune-netlist/src/expr.rs`). A árvore
  parseada vira uma arena (subárvores compartilhadas entre a função e
  as derivadas, como os ponteiros do C); as variáveis são os `v(n)` e
  `i(dev)` distintos em ordem de aparição. **`PTdifferentiate`
  literal**, inclusive a dobra dos construtores (`mkb`: constantes
  dobram com o operador cru — `/` sem fudge, `pow` puro —, depois as
  identidades 0/1; `mkf`: argumento constante dobra pela própria
  função do `ptfuncs.c`), com as derivadas do C (`tan' = 1/cos²`,
  `log' = log₁₀e/u`, `uramp' = u`, `u' = sgn' = 0`, `x^c` →
  `c·x^(c−1)·x'` só quando o expoente é constante na árvore —
  `x^(1+1)` e `x^-2` caem no caso geral). **`PTeval`**: `/` empurra
  o divisor para longe de 0 por `gmin` (`PTfudge_factor`), `^` com
  base negativa toma o expoente inteiro quando
  `|b−(int)b|/(b+0.001) < 1e-6` (e senão `|a|^b`), `sin/cos/tan`
  reduzem o argumento pelo `MODULUS` com `(int)`, e qualquer `HUGE`
  (`+∞`) devolvido é erro de avaliação. O load é o `ASRCload`:
  `V=` estampa o ramo (`b1#branch`, alcançável por `i(b1)`) e
  `−∂f/∂xᵢ` na linha do ramo; `I=` estampa `±∂f/∂xᵢ` nas linhas
  N+/N−; RHS `f(x₀) − Σ xᵢ₀·∂f/∂xᵢ`. **Sem limitação de junção**
  (o 3f5 não tem nenhuma para este dispositivo). O passe
  `MODEINITSMSIG` guarda as derivadas (`ASRCacValues`) que o AC e o
  `.pz` estampam (o `ASRCpzLoad` reavalia no op — mesmo número).
  `.disto`: o 3f5 (e o ngspice) não têm `DEVdisto` para a B — ela
  entra só pelas estampas lineares, como lá. Sem ruído (idem C). Sem
  `.sens` (falha alto, como todo dispositivo sem porta).

  Onde o ngspice-47 corrigiu o 3f5, o Dune segue o ngspice
  (KNOWN_DIFFS, gates medidos): `V=` constante estampa a linha do
  ramo (o `v_first` do 3f5 só estampa a partir da 1ª variável — `V=5`
  dá matriz singular; revertido, `bsrc_dc` falha "singular matrix"
  no `b8`); a derivada de `f^g` com `g` variável usa `ln f` (o 3f5
  escreve `ln(f')`; revertido, `bsrc_ac` FALHA a 500 e o `bsrc_dc`
  aborta); o teste de convergência avalia na solução **nova** (o do
  3f5 lê `CKTrhsOld` e nunca falha; sem efeito medido nas sondas);
  `srcFact` escala a fonte no TRANOP (D-0012, não exercitado);
  `ln`/`log` de exatamente 0 dão −1e99 (o −∞ do 3f5 envenena a 1ª
  iteração de todo `ln(v(n))`, que parte de v=0; revertido, um deck
  `v=ln(v(1))` aborta "not finite"); fonte `V=` com N+ = N− é erro
  (ngspice; no 3f5, singular). Mantém o 3f5: `log` base 10 (o ngspice
  faz `log` = ln — medido: `ln(2)+log(2)` dá 0.994 no Dune, 1.386 no
  ngspice), `|x|` em `ln/log/sqrt` (ngspice: erro), `x^y` com x<0 e y
  fracionário = `|x|^y` (ngspice: 0), `d(x^c)` = `c·x^(c−1)` (o
  ngspice usa `pwr`, de sinal trocado para x<0 e c ímpar), `exp`
  estourando é erro (ngspice satura em 1e99; sem efeito medido — o
  gmin stepping recupera nos dois). Avaliação não finita (NaN de
  `acos(2)`, −∞…) é falha de load (o C empurraria NaN para a
  matriz): o Newton trata como não convergência e o `CKTop` segue
  para os fallbacks.

  Fora (extensões do ngspice além do 3f5): `time`/`temper`/`hertz`,
  ternário e relacionais, `**`, as funções extras (`pwl`, `min`,
  `max`, `pow`, `pwr`, `ddt`, `floor`…), `tc1/tc2/m/reciproctc`,
  `.param` em expressões; e o `vmeas` do frontend do ngspice — para
  `i(l1)` (ou E, H, B) o ngspice insere uma fonte de 0 V em série
  (nó `5_vmeas_0`, vetor `v_l5#branch`); o Dune lê o ramo
  diretamente pelo `CKTfndBranch`, como o 3f5 (mesmo valor, sem o
  nó extra). Sondas: `bsrc_dc` (`.dc`, 8 fontes cobrindo todas as
  funções, `v(a,b)`, `i()`, `e`/`pi`, V= constante, carga
  exponencial resolvida pelo Newton) a 0.41; `bsrc_tran` a 0.003;
  `bsrc_ac` a 4.8e-7.

  **Pegas medidas ao escrever as sondas:** o ngspice-47 roda `.ac`
  antes de `.tran` qualquer que seja a ordem no deck (decks de sonda
  com os dois põem `.ac` primeiro); `ln`/`sqrt`/`acosh` exatamente na
  borda do domínio no chute inicial zero são frágeis no LU denso —
  o −1e99 de `ln(0)` contamina outras incógnitas em ~1e83 por
  arredondamento e um `asin` adiante dá NaN (o esparso do ngspice
  isola a linha); o ngspice força `maxIter ≥ 100` no `NIiter` e o
  Dune não (salvo nos degraus de fallback do `CKTop`, ver "Cadeia do
  op"), o que pesa para fontes B exponenciais sem limitação
  (descem ~vt por iteração).

Validação: `tests/op.rs` compara um deck-sonda (divisor, indutor,
diodo com RS, E, F, G, H, I) com números do ngspice-47 dentro das
tolerâncias SPICE; os decks `.op`/`.dc` de `corpus/basic/` passam no
oráculo contra o ngspice.

- **Orçamento de passos do `.tran`** (D-0022): nem o 3f5 nem o
  ngspice impõem o ITL5 que ambos parseiam (`cktsopt.c`:
  `case OPT_ITL5: break;`) — uma cascata de breakpoints do LTRA
  roda horas lá. O Dune conta toda tentativa de ponto (aceita ou
  rejeitada) contra `max(10 000, 50·⌈TSTOP/TMAX⌉)` (override:
  `Options.max_tran_steps`; corpus pica em 13 passos/vão) e falha
  com `Error::StepLimit`. Decks biestáveis no oráculo: flags
  `bistable`/`accept-error=` do `corpus/XFAIL` (formato de 5
  campos, ver docs/dune-oracle.md).

## Cadeia do op (`CKTop` do ngspice-47, 2026-11-08)

Contrato: `spicelib/analysis/cktop.c` e `optran.c` do ngspice-47 (o
3f5 só tem NR direto + as duas escadas fixas, `lib/ckt/cktop.c`).
Cada degrau só roda se todos os anteriores falharam; com o NR direto
convergindo nada novo executa (o corpus inteiro: 98 dos 101 ops
fecham no NR direto, saída byte-idêntica à de antes nos 102 decks).

1. **NR direto** (`cktop.c:40-51`): `NIiter(itl1)` a partir de
   `INITJCT`; `noopiter` pula.
2. **Gmin stepping** (`cktop.c:57-77`), com `gminsteps`:
   - `== 1` (default, `cktntask.c:121`): **dynamic gmin**
     (`dynamic_gmin`, `cktop.c:161-274`) e, se falhar e a variável de
     frontend `dyngmin` não estiver setada (default), **true gmin**
     (`new_gmin`, `cktop.c:349-466`). Os dois são o mesmo passeio
     adaptativo (`GminWalk`): zera solução e `state0`, começa em
     `1e-2/gminFactor` (`gminFactor = 10`), divide pelo fator a cada
     passo convergido — fator ×√fator (teto 10) se convergiu em
     ≤ itl2/4 iterações, `max(√fator, piso)` se em > ¾·itl2 —,
     pousa exatamente no alvo `max(gmin, gshunt)` (e o fator vira
     `gmin/alvo`), e numa falha volta ao último passo bom com o fator
     na raiz quarta, desistindo abaixo de 1.00005. Diferenças
     literais: o dynamic anda o `CKTdiagGmin` (piso 1.00005); o true
     anda o `CKTgmin` dos dispositivos com o diagonal em `gshunt`
     (piso 3). Desistindo ou não, os dois terminam com um NR completo
     a `itl1` do ponto onde pararam (`cktop.c:261`, `452`).
   - `> 1`: a escada fixa do 3f5 (`gmin·10^n`, ÷10 por passo,
     passo final com gmin 0 sempre tentado).
   - `0`: nenhum.
3. **Source stepping** (`cktop.c:85-92`), com `srcsteps`:
   - `== 1` (default, `cktntask.c:120`; o 3f5 tem 10): **Gillespie**
     (`gillespie_src`, `cktop.c:480-658`, `SrcRamp`): zera tudo,
     resolve com `srcFact = 0` (se falhar, escada fixa de 11 passos
     de `gmin·1e10` a `gmin` no diagonal, ainda com fontes a zero) e
     sobe `srcFact` de `raise` (0.001; ×1.5 após passo rápido, ×0.5
     após lento; numa falha ÷10 com teto 0.01, volta ao último fator
     convergido e o re-resolve). Desiste quando o passo falho estava
     a < 1e-8 do bom ou `raise < 1e-7`; sucesso = convergir em
     `srcFact = 1`, sem NR extra.
   - `> 1`: a escada `srcFact = i/n` do 3f5. `0`: nenhum.
4. **Op por transiente** (`OPtran`, `optran.c:290-852`;
   `cktop.c:94-103`): **ligado por default** — o `cp_init` do
   frontend executa `optran 1 1 1 100n 10u 0` (`frontend/init.c:
   77-93`; o comando `optran` em `.spiceinit`/`.control` é o único
   jeito de mudar ou desligar, não há `.option`). Marcha um
   transiente privado de 0 a 10 µs com `TSTEP = TMAX = 100 ns`, a
   partir do estado que o último degrau deixou: primeiro passo
   `min(10µ/100, 100n)/10`, cortado ÷10 de novo no início; Newton
   que falha (qualquer causa) corta ÷8 com ordem 1; LTE/ordem como
   `DCtran`; breakpoints só 0 e o final (`opbreaks`), com o ramo
   XSPICE do build do oráculo (descarte por `minBreak`, sem
   `MAX(delta, 2·delmin)`). O `CKTtime` **não avança** (só o `optime`
   local): as fontes ficam no valor transiente de t=0 e não plantam
   breakpoints; `delmin`/`minBreak` são os da análise chamadora (0
   no `.op`/`.dc`; `1e-11·TMAX` e `10·delmin` no TRANOP do `.tran`,
   `traninit.c:36`, `dctran.c:163`). O ponto final é o op.
   Verificado contra um ngspice-47 instrumentado: no `opfall_tran` os
   107 passos coincidem em tempo e ordem.

Detalhes do C que a cadeia exigiu e que valem para todos os degraus:

- **Modo INITF herdado.** `NIiter` move os bits INITF do `CKTmode` e
  uma chamada que falha os deixa onde parou (em geral `INITFIX`): o
  retry do degrau seguinte começa dali, não de `INITJCT`
  (`Engine::ni_iter_mode`). Cada degrau reinicia em `firstmode`.
- **Piso de 100 iterações** (`niiter.c:37-39`) nos degraus 2–4 (que
  passam `itl2`/`itl4` = 50/10 mas rodam 100); os limiares do passeio
  continuam usando `itl2` (12 e 37). O NR direto e o transiente
  normal mantêm os limites do 3f5.
- **`LoadGmin` sobre a diagonal pré-ordenada.** O Sparse 1.3 (solver
  default do ngspice-47 — o KLU só com `.options klu`; e do 3f5) roda
  `spMNA_Preorder` uma vez, na primeira matriz carregada: toda
  diagonal estruturalmente nula com um par de *gêmeos simétricos*
  `(r,j)`, `(j,r)` de módulo exatamente 1 (as incidências ±1 de um
  ramo de fonte de tensão) troca as colunas `j` e `r` — gêmeos únicos
  primeiro, depois a primeira coluna com vários, até não sobrar
  nenhuma. O `LoadGmin` então soma o gmin a `Diag[]` *depois* da
  troca: num ramo de fonte, aos dois ±1, e não ao nó. Efeito
  visível: sob gmin stepping o nó de uma fonte vale `V/(1+gmin)`
  (no schmitt a 5.6234e-3: −1.6 V → −1.59105 V, idêntico ao trace
  do ngspice) e esse nó não recebe shunt para a terra. O Dune antes
  somava em toda diagonal presente; agora `gmin_slots` emula o
  pré-ordenamento sobre o padrão e os valores da primeira carga.
  Nenhuma margem do corpus mudou.
- **Erro devolvido.** Se tudo falha, o `CKTop` do C imprime a causa do
  `OPtran` e devolve o código dele; o Dune devolve o erro do último
  degrau de Newton (singular, iterlim), mais útil que "timestep too
  small" num `.op`.
- **Linhas de transmissão** (`O`, `T`) pulam o `OPtran` (KNOWN_DIFFS):
  com o `CKTtime` congelado o histórico de convolução não faz
  sentido, e o accept das linhas não roda no op.

Sondas (`corpus/regress/`, golden ngspice-47, verificadas também com
`set ngdebug` e um build instrumentado do ngspice-47 que imprime o
modo INITF e as iterações de cada `NIiter` da cadeia):

| deck | degrau que fecha (Dune = ngspice) | mecanismo |
|---|---|---|
| `opfall_src` | Gillespie source stepping (17 passos, 0.1% → 100%, mesma sequência) | diodo `off` reverso atrás de fonte DC: o `INITFIX` segura vd=0 e o `DIOconvTest` falha a cada iteração; só com a fonte a zero vd=0 é solução |
| `opfall_tran` | `OPtran` (107 passos, mesmos tempos) | o `.op` do offbjt: a fonte PULSE não tem `dc`, então no `MODEDCOP` fica no valor de t=0 sem `srcFact` e o source stepping não a zera; o BJT `off` trava no `INITFIX` em todos os degraus de Newton |
| `opfall_gmin` | true gmin | cadeia flutuante de diodos `off` atrás de 1000 V: o gmin diagonal puxa os nós para a terra e põe d1 na ruptura; o gmin dos dispositivos os deixa no trilho. **Ressalva:** o que tira o NR do `INITFIX` aqui é um teste de corrente decidido pelo arredondamento do LU (delvd ~1e-9 V × gd) — o ngspice sai em 3 iterações a 5.6e-3, o Dune passeia mais; os dois fecham no true gmin com o mesmo valor (margem 0.001), mas o degrau pode variar entre plataformas. O teste unitário só pina a resposta. |
| `offbjt` (pré-existente, `.tran`) | Gillespie (antes: escada do 3f5) | no TRANOP o `srcFact` escala o PULSE |

Não consegui construir um deck *robusto* que só feche no true gmin:
o que o distingue do dynamic é a ausência de shunt nó-terra, e os
casos achados (busca aleatória de ~3300 circuitos contra o ngspice;
`.claude/scratch/opfall/search/`) ou são decididos por arredondamento
como o `opfall_gmin`, ou têm múltiplas soluções. O passeio em si
está pinado por testes unitários que refazem as sequências de gmin do
ngspice (`gmin_walk_retraces_ngspice_schmitt`, `…_gives_up_like_ngspice`,
`gmin_walk_slow_floor`) e a rampa do Gillespie
(`src_ramp_retraces_ngspice`, `src_ramp_backs_off`).

Caminhos do corpus que passam pelos fallbacks: schmitt e mosamp2
(dynamic gmin) e offbjt (Gillespie). O mosamp2 refaz o passeio do
ngspice passo a passo (1e-3 → 1e-12 em décadas); o schmitt segue o
mesmo mecanismo mas, num latch que parte do metaestável, um modo
instável amplifica o arredondamento (valores ~1e-14 V que diferem no
4º dígito já na 1ª iteração): o 2º passo leva 13 iterações no Dune e
11 no ngspice, o que muda o fator e a sequência de gmins dali em
diante — mesmo op final, margens inalteradas.

## `.options trytocompact` (LTRA, 2026-11-08)

`ltraset.c:55-72` e `ltraacct.c:84-127, 262-285` (idênticos no 3f5
e no ngspice-47 salvo realocação de memória). Duas coisas:

1. Modelo sem `lininterp`/`quadinterp`/`mixedinterp` interpola
   **linear** em vez de quadrático, com o aviso do C
   ("using linear interpolation because trytocompact option
   specified") nas notas.
2. Após cada ponto aceito com ≥ 3 amostras, se as três últimas de
   `v1`, `v2`, `i1`, `i2` de **todas** as linhas estão numa reta
   (`LTRAstraightLineCheck`: área do trapézio contra a soma das duas
   áreas, com `COMPACTREL`/`COMPACTABS`, default `RELTOL`/`ABSTOL`),
   a amostra do meio sai do histórico de todas (no C, a lista global
   `CKTtimePoints` e o `CKTtimeIndex--`). O teste roda antes da regra
   de inclinação do accept, que vê o histórico ainda cheio.

No Dune cada linha tem a própria cópia da lista de tempos; o motor
coordena (`Device::compact_ready`/`compact`, chamados em
`Engine::accept`): compacta só se toda linha participante está
pronta. Fora do modo `ltrafit` (recursivo, que tem estado próprio
por ponto) e do `.pss` (passado periódico semeado). Opt-in como no C
(`Options::trytocompact`, default false).

Sondas: `regress/ltra_compact` (RLC, PULSE com patamares longos,
`trytocompact`) e `regress/ltra_lin` (o mesmo com `lininterp` e sem
a opção — diferem só pela compactação), ambas em lockstep com o
ngspice-47 (6.7e-15 e 7.6e-15). Medido (`trytocompact_shrinks_the_ltra_history`):
com a opção, 1112 pontos aceitos, 1100 compactações e histórico
final de **12** amostras; sem, 578 pontos e histórico de 578. A
compactação muda a malha de passos (mais pontos, porque a regra de
inclinação passa a ver retas longas) — e o ngspice dá os mesmos 1112.

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
   −3·n·vt; a cadeia de fallbacks do `CKTop` do ngspice-47 como
   default (dynamic/true gmin, source stepping de Gillespie, `OPtran`;
   as escadas do 3f5 vivem em `gminsteps/srcsteps > 1`);
   `srcFact` escalando toda fonte no TRANOP. Exceção na outra
   direção: o corte do MOS2 fica na fronteira `von` do 3f5 (o
   `vbin` do ngspice produz gm negativo com corrente nula e só
   converge com o dynamic gmin de lá; correntes idênticas).

## Caminho único cartão → análise (`Circuit::run_card`, 2026-11-08)

`run.rs` é o único lugar que lê um cartão de análise do IR e o
traduz em chamada do motor. `Circuit::run_card(&Analysis, &Options)
-> Result<CardOutput, Error>` devolve `CardOutput::Plots { plots,
notes }` para todo cartão exceto `.pss`, e `CardOutput::Pss(Box<Pss>)`
para o `.pss` — o resultado inteiro (período, `t0`, histórico do
shooting), que os plots sozinhos não carregam. `Circuit::run` é
`run_card(..).map(CardOutput::into_plots)` (o `.pss` vira `[tempo,
espectro]`, como antes); `Session::run_card` é o mesmo com o warm
start e a re-rodada fria após um miss do `Session::run_analysis`.
O `keepopinfo` mora no `run_card` (não se aplica ao `.pss`).

Antes, o wasm repetia a tradução cartão → `PssParams` (truncamento
IF_INTEGER das contagens, defaults) para obter o `Pss` inteiro, com
um teste-trava comparando as duas cópias; e a sua `Session` tinha uma
re-rodada fria própria para o `.pss` (desligava e religava o warm
start, esquecendo o ponto guardado). Agora o wasm chama
`run_card`/`Session::run_card`; a trava virou
`tests/run.rs::run_card_is_the_one_card_mapping` (contagens
fracionárias truncadas, defaults, contagem negativa = erro, `run` =
`run_card` = `Session::run_card` = `Session::run_analysis`, e o plot
do `keepopinfo`). Python e R não duplicam: o `.pss` dos decks passa
pelo `Circuit::run`, e `spice.pss()`/`spice_pss()` montam `PssParams`
dos próprios argumentos, não de um cartão.

Superfície pública nova (ponto de pausa, aguarda aprovação): o enum
`CardOutput` (reexportado na raiz), `CardOutput::into_plots`,
`Circuit::run_card` e `Session::run_card`. `pub(crate)` não basta
porque o consumidor é outro crate (`dune-wasm`).

## Cartões e sweeps menores (2026-11-08)

- **R com modelo** (`restemp.c`, `resmpar.c`, `resparam.c`): modelo
  `r` com `RSH DEFW NARROW TC1 TC2 TNOM`; sem valor,
  `R = RSH·(L−NARROW)/(W−NARROW)` (W default DEFW = 10e-6 — o
  código; o `spice.txt` diz 1e-6), e sem RSH ou L o aviso do C
  "resistance=0, set to 1000". Com ou sem valor, a condutância é
  `1/(R·factor)`, `factor = 1 + TC1·ΔT + TC2·ΔT²`, `ΔT = T − TNOM`,
  `T` = `TEMP=` da instância, senão a do circuito mais o `DTEMP=`
  do ngspice. O fator é recalculado em `ResistorI::update_conduct`
  (o `RESupdate_conduct` do ngspice: `g = m/(R·factor·scale)`, AC
  idem) a partir de `ResTemp {tc1, tc2, tnom, temp, dtemp}`, que
  vem da elaboração por uma tabela lateral crate-privada
  (`Circuit::res_temp`, como o `ic_given`) — **o spec público
  `Resistor` não mudou** (ponto de pausa de API: a extensão do spec
  tipado está proposta em `docs/fase4-api.md`, item 7). R sem
  modelo e sem valor segue erro (o 3f5 daria 1 kΩ calado).
- **C com modelo** (`captemp.c`, `capsetup.c`, `capmpar.c`): modelo
  `c` com `CJ CJSW DEFW NARROW`; sem valor,
  `C = CJ·(W−N)·(L−N) + 2·CJSW·((L−N)+(W−N))`, L default 0. Puro
  na elaboração (o 3f5 não tem temperatura no C).
- Divergências do ngspice-47 na geometria (SHORT/2·NARROW), no
  fallback (1 mΩ) e os parâmetros só-ngspice (ignorados com nota)
  estão no KNOWN_DIFFS. Sondas `regress/rmodel` (.op, margem 0) e
  `regress/cmodel` (.ac, margem 0), ambas com NARROW=0; a fórmula
  do 3f5 com NARROW≠0 e o fallback ficam em `tests/cards.rs`.
- **`.dc` além de fontes.** Contrato medido no C: o `DCtrCurv` do
  3f5 (`ckt/dctrcurv.c:67-111`) só procura `Vsource` e `Isource`
  ("DCtrCurv: source %s not in circuit", `E_NODEV`) — nem TEMP nem
  parâmetro de modelo. O ngspice-47
  (`spicelib/analysis/dctrcurv.c:89-157`) procura resistor
  (`rcode`: varre `RESresist` com `RESresGiven`, chama
  `RESupdate_conduct` — o fator de temperatura continua dividindo),
  depois V, I e por fim a palavra `temp` (`TEMP_CODE`: `CKTtemp =
  valor + CONSTCtoK` e `CKTtemp(ckt)` a cada ponto; o passo que já
  passou do stop não reavalia, linha 457); escalas `res-sweep` e
  `temp-sweep`, tipadas pelo próprio nome. Parâmetro de
  modelo/instância não é variável de `.dc` em nenhum dos dois
  (no ngspice só por laço `.control`/`alter`). O Dune segue o
  ngspice (KNOWN_DIFFS): `SweepTarget::{Device, Temp}`; o nível de
  temperatura reconstrói os dispositivos com `opts.temp` novo
  (`Circuit::build`) e troca a lista no motor
  (`Engine::replace_devices` — mesma ordem, nós e estados, slots
  re-derivados num builder novo), reaplicando os valores correntes
  dos outros níveis; `TEMP=` de instância fica. Sonda
  `regress/dctemp` (`.dc temp -25 125 25 v1 1 3 1`: diodo, BJT, R
  com TC e um R com `temp=` fixo) PASS a 0.475.
- **`keepopinfo`** (`cktsopt.c`, IF_FLAG): `Options::keepopinfo`;
  `Circuit::run` antepõe o op (`self.op`, idêntico ao op da
  análise) com o nome do C — 3f5: `.ac` "AC Operating Point"
  (`acan.c:73`), `.pz` e `.disto` "Distortion Operating Point"
  (`pzan.c:46` herdou o nome por copy-paste; `distoan.c:126`).
  ngspice-47 acrescenta `.noise` ("NOISE Operating Point",
  `noisean.c:221`) e `.sp` ("AC Operating Point", `span.c:476`),
  que o Dune segue (KNOWN_DIFFS). Plot real, sem escala. Sonda
  `regress/keepop` (PASS 0.012 / 0.005); pz/disto/noise medidos à
  mão contra o ngspice (PASS) e pinados em `tests/cards.rs`.

## Temperatura de instância (2026-11-08)

- **Quem tem `TEMP=` de instância no 3f5** (tabelas `IFparm` com
  `IOPU("temp")`): `res/res.c:15`, `dio/dio.c:14`, `bjt/bjt.c:71`,
  `jfet/jfet.c:22`, `mos1/mos1.c:26`, `mos2/mos2.c:37`,
  `mos3/mos3.c:38`, `mos6/mos6.c:37`. BSIM1/BSIM2 têm `temp` só como
  parâmetro **de modelo** (`bsim1/b1.c:97`, `bsim2/b2.c:163`, em °C;
  o Dune já lia); MESFET, chaves, linhas e fontes não têm. O Dune já
  lia todos os oito (specs `temp`, `*Temp::new(…, temp, tnom)` refaz
  o `*temp.c` da instância); faltava sonda `.op`/`.dc` para BJT,
  MOS1, MOS2 e MOS6: `regress/itemp_dc` (gêmeos quentes/frios contra
  gêmeos na temperatura do circuito, `.options temp=40`) PASS 0.609;
  sem o `TEMP=` dos dispositivos FALHA de 78 a 498. O sweep começa em
  0.65 V: abaixo disso o BJT cai no nível de gmin e aparece o gmin de
  substrato do ngspice (KNOWN_DIFFS, já conhecido). Correção de
  fidelidade de passagem: `TEMP=`/`DTEMP=` num M de BSIM1/BSIM2 era
  descartado calado; agora é "unknown parameter", como no 3f5 e no
  ngspice (medido) e como o BSIM3 já fazia.
- **`DTEMP=`** (só ngspice-47; `IOPU("dtemp")` em `res.c:18`,
  `dio.c:15`, `bjt.c:74`, `jfet.c:23`, `mos1.c:28`, `mos2.c:39`,
  `mos3.c:40`, `mos6.c:38` — e em `cap.c`/`ind.c` e dispositivos fora
  do contrato; MES, BSIM1/2/3 não têm). Semântica do C (`diotemp.c:
  291-294`, `bjttemp.c:75-79`, `jfettemp.c:83-88`, `mos1temp.c:129-
  134` e iguais): `if (!tempGiven) temp = CKTtemp + dtemp` — `TEMP=`
  ganha quando os dois aparecem; o `restemp.c:35-49` ainda zera o
  `RESdtemp` e avisa "Instance temperature specified, dtemp ignored"
  (o Dune emite a mesma nota). A diferença é em kelvin/°C, sem
  offset (`*param.c`). Implementado sem mexer nos specs públicos:
  `Circuit::dtemp` (nome → ΔT) alimentado pelo `elaborate.rs`, lido
  por `Circuit::instance_temp` nos sete construtores; o R leva o
  `dtemp` no `ResTemp`. Como o `.dc temp` reconstrói os
  dispositivos com o `CKTtemp` novo, a instância com `DTEMP`
  acompanha a varredura (a com `TEMP=` não). `dtemp` virou keyword
  no parser de R/C (como `temp`: não é nome de modelo). C/L seguem
  sem temperatura (nota "ignored"). Sonda `regress/dtemp_op` (os oito
  dispositivos + `TEMP=`/`DTEMP=` juntos no R e no diodo) PASS 0.387;
  sem `DTEMP` FALHA de 32 a 495; com `DTEMP` ganhando FALHA a 132/500.
- **Ruído do R** e **`.sens` do R**: ver as seções de ruído e de
  `.sens` acima (contratos com arquivo/linha; sondas
  `regress/rnoisetemp`, `regress/sens_rtemp`).

## Pendências (fases seguintes)

- Variantes do ngspice não modeladas: `ngbehavior=spice3` (forma
  3f5 do flicker MOS); `XPART` do BSIM1 como flag (`IF_FLAG`:
  `xpart=0` presente já liga 0/100, medido) — o Dune decide pelo
  valor (`≥1`), ver KNOWN_DIFFS, e as sondas evitam `xpart=0`.
- ~~`DTEMP` de instância~~, ~~`.sens` dos parâmetros de temperatura
  do R~~, ~~ruído térmico do R na temperatura da instância~~ — feitos
  em 2026-11-08 (seção "Temperatura de instância"). Fica: o `DTEMP`
  e o modelo/`TEMP=` do R nos specs tipados (proposta em
  `docs/fase4-api.md`, item 7); gmin do ngspice na junção de
  substrato do BJT (KNOWN_DIFFS). Espelho de `keepopinfo` nas listas
  de override dos bindings (Py/R/wasm).
- JFET nível 2 (Parker–Skellern) e MES 2/5/6 (MESA/HFET) são
  ngspice, fora do contrato 3f5. (Linha U e fonte B entraram em
  2026-11-08 — seções próprias.)
- Pz das chaves sem golden (só testes unitários); o ruído está
  pinado por `regress/fetnoise` (`onoise_s1`).
- ~~LTRA: `trytocompact` e o controle LTE~~ — `trytocompact`
  portado (2026-11-08); o controle LTE (`LTRAlteCalculate`) é
  inalcançável nas duas tabelas de parâmetros (seção LTRA acima).
- Controlador de passo: cruzamento de `vds=0` num breakpoint com nó
  de ~1 fF mata o passo no BSIM1/2 (ver o caso-limite acima).
- Bypass de dispositivos (`CKTbypass`) — otimização, não semântica.
- ~~Fallbacks restantes do op do ngspice~~ — a cadeia inteira entrou
  em 2026-11-08 ("Cadeia do op"). Fica: espelhar `trytocompact` nas
  listas de override dos bindings (Py/R/wasm), como `keepopinfo` (o
  cartão `.options` já funciona em todos).
- Ruptura do diodo diverge do ngspice-47 com corrente alta (medido ao
  construir as sondas do op: `i1 0 1 1.6; d1 0 1 (is=6.79e-18
  n=1.47 bv=50)` dá 50.70 V no Dune e 50.32 V no ngspice; não é o
  `NBV`, cujo default no ngspice é `N`, `diosetup.c:131`). Não
  investigado; nenhum deck do corpus opera na ruptura forte.

## Transiente robusto (opt-in, issue #6)

Nenhum dos dois é contrato 3f5 (D-0002 vale só para trap/gear); os
defaults são fiéis, e com as opções desligadas o binário produz
rawfile e stdout **byte-idênticos** ao HEAD anterior nos 76 decks
do corpus (verificado via `git archive`). Decisão D-0027.

- **`.options method=trbdf2`** (`Method::TrBdf2`): composto TR-BDF2
  (Bank et al. 1985; Hosea & Shampine 1996) — cada passo h é um
  estágio trapezoidal até γ·h (γ = 2−√2) e um BDF2 não-uniforme em
  {t, t+γh, t+h}, ambos sobre o `com_cof` existente; 2ª ordem,
  L-estável (o trap amplifica com −1 quando h/τ→∞ e ringa; o
  composto amortece). Onde o 3f5 corta a ordem (1º ponto, pós-
  breakpoint, retry), o passo é backward Euler com `CKTterr` de
  ordem 1 — necessário: a corrente de companheiro guardada em t
  pode ser anterior a uma quina de fonte. LTE do composto: Bank,
  `2|k|h³|q'[·,·,·]|` com k=(−3γ²+4γ−2)/(12(2−γ))≈−0.0404, na
  MESMA escala do CKTterr (1/6 do erro verdadeiro nas ordens 2).
  Bancos de estado e `delta_old` giram por estágio (`unrotate` na
  rejeição); dispositivos com histórico próprio (TRA) leem
  `step_old` (= passos aceitos). Custo: 2 Newtons/passo — em deck
  suave, trap a TMAX/2 empata; o valor do trbdf2 é a
  L-estabilidade. Medidas (ngspice gear a TMAX/100 como
  referência): ltra_3 completa com 763 pontos e v(x3.2) rms 3.7 mV
  (trap do Dune: 23 mV; golden ngspice: 65 mV); erro ≤ trap em
  rc/trrlc/truic (≈0.5×, a razão 0.0404/0.0833 das constantes);
  regressão `corpus/regress/trbdf2_ring.cir` (fileira do dioload:
  trap alterna 164×, trbdf2 10× — deck em corpus/SKIP porque o
  ngspice-47 não tem o método e o golden seria um trap ringando).
- **`.options ltrabkrate=N`** (`Options.ltra_break_rate`): higiene
  dos breakpoints da regra de inclinação do `ltraacct.c`. O
  diagnóstico (fase 0): o `CKTminBreak` (5e-5·TMAX) é guarda de
  coincidência, não teto de densidade — admite ~20k breakpoints
  por vão de TMAX e a cascata do ltra_3 vive a 4,6× o minBreak.
  Com N>0, pedido da regra de inclinação a menos de
  max(TMAX/N, minBreak) de um breakpoint já agendado é descartado
  (~N por vão). Sem cascata é bit-idêntico (medido em
  ltra_1/2/3/rc/rg no arm64). Limita o custo; a correção de
  verdade é o trbdf2 (trap+N=10 completa o ltra_3 no linux mas o
  ring persiste, v(5) erra 0.185 V).
- **`.options ltrafit`** (`Options.ltra_fit`, issue #8): convolução
  recursiva do LTRA. Núcleos sem o atraso viram somas de
  exponenciais reais por quadratura Gauss–Legendre graduada da
  integral exata no corte de ramo (RLC: x=2α·sin²θ; RC: x=y²,
  janela híbrida RCl²/8); polos reais estáveis por construção;
  modelo montado no accept do op para o horizonte TSTOP; orçamento
  1e-6 (máx e L1) por núcleo contra a forma fechada com Bessel
  precisa, reportado em nota. Estados avançam só em ponto aceito
  (nunca no estágio γ do TR-BDF2); primeiro segmento exato
  (coeficientes do C); breakpoints/pnjlim/steplimit/interpolação
  inalterados; load O(1). Desligada: byte-idêntico nos decks LTRA.
  Medidas: 24 polos/linha no ltra_3 com fit ≤2,3e-11; ltra_1
  melhora a margem (0,038 vs 0,061); trbdf2+fit erra 0,99–1,03× o
  trbdf2 puro na referência fina; 20× na sonda de 20k pontos, 6×
  no ltra_3 trbdf2, 17× no caso trap+ltrabkrate. Diferença
  deliberada do modo: com aux==0 o C descarta o 1º segmento
  atrasado; a recursão o inclui. Sonda corpus/regress/ltrafit.cir
  (oráculo passa a 76).
- **`Options.tmax_divisor`** (issue #16; argumento de API, não
  chave de `.options` — o TMAX é parâmetro do cartão `.tran`, e o
  divisor vale para todo cartão `.tran` do deck): `Some(d)`, d ≥ 1
  finito, divide o TMAX efetivo (o 4º argumento, ou o default
  `min(TSTEP, span/50)`); `delmin`, raio de merge de breakpoints e
  orçamento de passos seguem. Resultado bit-idêntico ao deck com o
  cartão editado para TMAX/d (teste `tests/tmax_divisor.rs`, rc e
  rlc). Fora da faixa: `Error::Build` no `.tran`. Exposto nos
  bindings como `tmaxDivisor` (JS) e `tmax_divisor=` (Python/R) —
  o "passos finos" do Studio (rpic-studio#19).

## Regime permanente periódico — `.pss` (issue #13, trilha 4 de D-0028)

Análise nova, sem contrato 3f5. O ngspice-47 tem um `.pss`
(`analysis/dcpss.c`, Perticaroli/Lannutti, `--enable-pss`, ativo no
binário do Homebrew), mas o algoritmo é uma **busca de oscilador
livre**: roda o transiente e, a cada ciclo, re-estima a frequência
pela derivada do vetor solução perto do mínimo de
`|x(t) − x(t_ref)|`; nunca resolve `x(T) = x(0)`, e todos os
exemplos dele (`examples/pss/`) são osciladores. Num circuito
excitado ele desafina do estímulo: medido no ngspice-47 com os decks
de `corpus/pss/` — RC a 1 kHz sai em 750 Hz, RLC a 1,5 kHz em
1406 Hz, trem de pulsos de 1 MHz em 936 kHz; só o retificador cai no
estímulo (50,000002 kHz). O Dune mantém o **cartão e os nomes de
plot do ngspice** e troca o método.

**Cartão** (`dot_pss` do `inp2dot.c`, mesma ordem posicional):
`.pss FREQ TSTAB OSCNODE <POINTS <HARMS <SCITER <STEADYCOEFF>>>> <UIC>`.
No ngspice tudo até STEADYCOEFF é obrigatório (o que falta vira lixo);
no Dune o rabo é opcional (defaults 128, 10, 50, 1). `FREQ = 0`
colhe a fundamental das fontes (MDC das frequências: SIN/SFFM com
frequência explícita, PULSE `1/PER`; EXP/PWL/PULSE sem PER assentam
em constante e não contam; SIN amortecido ou sem frequência é erro).
`OSCNODE` é conferido; num circuito excitado só é reportado, num
autônomo (nenhuma fonte periódica) carrega a condição de fase — ver
"Osciladores autônomos" abaixo.
`HARMS` conta o DC, como no ngspice. Semântica do STEADYCOEFF difere
(ver convergência). `Circuit::pss(&Options, &PssParams) -> Pss`.

**Método — shooting–Newton matrix-free.** `Φ(x₀)` = estado em
`t₀+T` do transiente que parte de `x₀` em `t₀`; resolve-se
`F(x₀) = Φ(x₀) − x₀ = 0` por Newton, `(I − Φ′)δ = Φ(x₀) − x₀`. A
monodromia `Φ′` nunca é montada: GMRES (sem restart, MGS com
reortogonalização) sobre as incógnitas escaladas `x_i/tol_i` pede só
produtos `Φ′v`, cada um uma diferença direcional de dois transientes
(Telichevesky–Kundert–White 1995). Escolhas:

- **Diferenças vs sensibilidade direta**: a sensibilidade exigiria
  de cada dispositivo `∂estado/∂x` do histórico (o que o C não
  tem); as diferenças reusam o transiente inteiro sem tocar
  dispositivo. Custo por passo de Newton: 1 transiente adaptativo +
  1 replay de base + k replays (k ≈ modos lentos + 1, contra n de um
  jacobiano denso por diferenças). Linear converge em 1 passo.
- **Grade congelada**: o resíduo vem de um transiente adaptativo
  (LTE, breakpoints, trap/gear/trbdf2, orçamento D-0022 — o
  `tran_loop` de sempre, só que partindo de `x₀`), cujos passos
  aceitos são registrados (tempo, ordem, flag `CKTbreak`). Os
  produtos `Φ′v` refazem essa grade (`Engine::tran_replay`, sem LTE
  nem escolha de passo): uma corrida adaptativa perturbada escolheria
  outros passos e o ruído do controle de passo, dividido por ε,
  destruiria a derivada. Os replays apertam RELTOL/VNTOL/ABSTOL por
  1e-3 (o erro de Newton por passo também seria amplificado); se um
  replay não converge, o passo de Newton é refeito com as
  tolerâncias do deck. ε = 1e-2 tolerâncias. Passo de Newton que não
  reduz o resíduo é cortado à metade até 4×.
- **Estado de partida**: cada corrida é um engine novo (históricos de
  dispositivo limpos); `seed_states` faz passes de load em
  `MODETRANOP|MODEINITFLOAT` sobre `x₀` fixo, sem solve, até nenhum
  dispositivo limitar junção — cargas, `vd` etc. viram funções de
  `x₀`, como o `.op` deixaria. O vetor MNA inteiro é o estado do
  shooting; as componentes algébricas saem sozinhas (coluna de `Φ′`
  nula). O primeiro passo é o BE de `MODEINITTRAN`, como no `.tran`.
- **Intervalo**: `t₀` = primeiro múltiplo de T em ou após TSTAB (pelo
  menos 1 período de estabilização, `.tran` normal, com UIC se
  pedido); quinas de PULSE/PWL dentro de `(t₀, t₀+T)` semeiam a
  tabela de breakpoints (`Device::wave_corners`: o `VSRCaccept` só
  planta a próxima quina quando o ponto já está numa). TSTEP = T/POINTS
  (default de TR/TF do PULSE), TMAX = min(TSTEP, T/50): **POINTS é o
  botão de exatidão** (o erro medido é a fase do trapezoidal,
  (ωh)²/12).

**Convergência**: `max_i |x_i(t₀+T) − x_i(t₀)| / tol_i ≤ STEADYCOEFF`,
`tol_i = RELTOL·pico_i + VNTOL` (nós) ou `+ ABSTOL` (ramos), `pico_i`
o maior `|x_i|` no período — o `NIconvTest` com a escala do sinal
(o `x_i(t₀)` de uma senoide pode ser ~0). No ngspice o STEADYCOEFF
multiplica `TRTOL·(RELTOL·excursão + VNTOL)` de outro teste; aqui é
só a escala do critério (1 = tolerâncias SPICE). Falha =
`Error::PssConvergence { iterations, residuals }` com o histórico;
toda corrida bem-sucedida deixa nota com o histórico, contagem de
transientes e dimensões do GMRES.

**Saída**: dois plots com os nomes do ngspice —
`Time Domain Periodic Steady State Analysis` (escala `time`,
POINTS+1 linhas equiespaçadas de `t₀` a `t₀+T`, interpoladas da grade
aceita por Lagrange cúbica sem atravessar breakpoint) e
`Frequency Domain Periodic Steady State Analysis` (escala
`frequency`, HARMS linhas `k·f`). O espectro do Dune é **complexo**
(`X_0` média; `x(t) = X_0 + Σ Re(X_k e^{jkωt})`, a convenção de
fasor do `.ac`, fase referida a t=0): `mag()` dele é o plot real do
ngspice, que só escreve magnitudes. `.print pss` imprime o plot do
tempo.

**Validação** (`crates/dune-core/tests/pss.rs`, decks e goldens em
`corpus/pss/`, `tools/pss-golden.sh`; fora do `--ci`, que segue em
76):

| Deck | Referência | Resultado |
|---|---|---|
| `pss_rc` (RC 1 kHz, ωRC=2π) | forma fechada `|H| sin(ωt+∠H)` | default: erro máx 2,0e-4 da amplitude, X₁ 2,0e-4 (= (2π/128)²/12); resíduo 4,2 → 3,9e-10 (1 Newton, GMRES dim 1). RELTOL 1e-7 + 1024 pts: 3,1e-6 (= (2π/1024)²/12). gear 8,0e-4; trbdf2 9,7e-5 |
| `pss_rlc` (Q=10, τ=2 ms, parte a 1 período) | forma fechada | resíduo 360 → 4,7e-6 (1 Newton); erro 6,1e-4 default, 3,8e-5 apertado (fase do trap ×Q perto da ressonância) |
| `pss_rect` (trdiode) | cauda de `.tran` 200 períodos | vs ngspice: 4,06 mV em v(2) (régua D-0010 59 mV); vs `.tran` do Dune no mesmo passo: 0,40 mV; ambos apertados: 12 µV. Resíduo 3,9 → 4,5e-3 |
| `pss_rect` | `.pss` do ngspice-47 (único deck em que ele acha o estímulo) | harmônicos: máx \|Δ\| 9,0e-5 V em v(2) (DC 4,037315 vs 4,037225; H1 0,1272230 vs 0,1272195); i(vin) 1,4e-6 A |
| `pss_pulse` (pulsos TD=0,3 µs, grampo a diodo) | cauda de `.tran` 300 períodos | vs ngspice: 0,40 mV v(2), 17 µV v(3) (réguas 25/17 mV); resíduo 374 → 3,2e-3 |

Rawfile e stdout dos 83 decks do corpus (examples, basic, regress,
measure) **byte-idênticos** ao main com o refactor do `DCtran`
(`tran_loop` extraído; log de passos opcional).

**Limitações/pendências**: linhas de transmissão entram pelo
caminho próprio descrito em "Linhas de transmissão no `.pss`" abaixo
(as RLC/RC do LTRA exigem `.options ltrafit`); chaves com histerese
semeiam o estado pelo controle (dentro da banda pode escolher o ramo
errado); ruído transiente fica para a outra leva da issue #13.

### Osciladores autônomos (issue #20)

**Detecção.** Um deck sem fonte periódica (nenhuma SIN/SFFM com
frequência, nenhum PULSE com PER — a mesma colheita do `FREQ = 0`)
é um oscilador livre: o período vira **incógnita** e resultado
(`Pss::period`; `Pss::periods` traz o `T` de cada iteração, alinhado
com `residuals`). OSCNODE e TSTAB > 0 passam a ser obrigatórios (o
cartão já exige o nó). `FREQ ≠ 0` é chute; `FREQ = 0` estima. Um
deck só com fontes DC e `FREQ` dado, que antes rodava como
"excitado" e convergia na constante, agora é recusado com erro claro
se não oscila (é o que ele é).

**Partida (kick).** O Dune não injeta perturbação: o `.tran` de
estabilização é o de sempre, e o deck tira o circuito do equilíbrio
instável como na bancada — `.ic` + `UIC`, ou rampa de alimentação
(PWL, ou PULSE sem PER, que não conta como periódico). É também a
convenção do ngspice-47: o `dcpss.c` não tem kick, e todos os
exemplos de `examples/pss/` partem de rampa PWL na fonte (`ring`,
`colpitt`, `hartley`) ou de `ic=` com `uic` (`vdp`). Contar com o
ruído numérico seria desonesto: num equilíbrio simétrico exato (anel
de inversores idênticos) o erro de arredondamento pode levar
centenas de constantes de tempo, ou nunca, para crescer. Se a janela
não oscila, o erro diz isso e sugere o kick.

**Estimativa de T** (janela = última metade do TSTAB). Contador de
frequência: cruzamentos **ascendentes** de OSCNODE pelo nível
médio-de-faixa `(max+min)/2`, T = espaçamento médio dos últimos 4
períodos (dispersão reportada). Escolhido contra a autocorrelação
porque (i) o cruzamento é onde `|dv/dt|` é máximo, então o instante é
o mais bem condicionado do ciclo e a interpolação linear basta; (ii)
vale para qualquer forma de onda (senoide, quadrada de anel, pulso de
relaxação) sem reamostrar em malha uniforme; (iii) o pico da
autocorrelação é largo (curvatura ~ω²) e enviesado pela amplitude
ainda crescendo; (iv) é o mesmo ponto que serve de partida da
condição de fase. O médio-de-faixa (não a média) garante cruzamento
mesmo em forma de onda assimétrica; é tomado sobre os últimos
4·T estimados, para que a amplitude ainda crescendo no começo da
janela não o desloque. Sem FREQ, a estabilização roda com
TMAX = TSTAB/1000 (o LTE refina) e a estimativa é **refinada** numa
continuação de 5,5 períodos na malha do shooting (T/POINTS) a partir
do estado final — Colpitts 5,2% → 7,7e-4, anel 3,3% → 4e-6 —, o que
poupa iterações de Newton (Colpitts: 4 → 2). Com FREQ, a
estabilização já roda na malha T/POINTS do chute; se a janela tem ≥3
cruzamentos, **o T medido vence o chute** (nota "FREQ's guess was");
o chute só semeia T quando não há o que medir. Medido: da partida com
1,2 período de TSTAB (um cruzamento), o Newton traz T de ±10% em 5
iterações (vdp; +20% falha com 1,2 T e converge com 2,2 T).

**Método — Newton aumentado em [x₀; T].** Resíduo
`[Φ_T(x₀) − x₀ ; x₀[osc] − v_ref]`. Condição de fase escolhida:
**valor fixo** — OSCNODE preso no valor que tinha no último
cruzamento ascendente do médio-de-faixa (o `t₀` reportado). Contra a
derivada nula (`ẋ_osc(t₀) = 0`, pico): (i) a linha de borda do
jacobiano é `e_oscᵀ`, constante, então a condição é satisfeita
exatamente em toda iteração e sai do sistema — `δx[osc] = 0` e T
ocupa o lugar de `x[osc]` entre as incógnitas: o sistema volta a ser
quadrado, `n − 1` por `n − 1`, sem linha nova; (ii) a matriz
aumentada é não singular sse `ẋ_osc(t₀) ≠ 0` (transversalidade), e
no cruzamento do médio-de-faixa `|ẋ|` está perto do máximo — a
condição de derivada nula fica justamente onde `ẋ_osc = 0` e, numa
onda quadrada (anel), o patamar tem derivada nula num intervalo
inteiro: a fase não é presa; (iii) a derivada nula exigiria `f(x₀)`
(uma avaliação de dispositivo e sua derivada no produto de GMRES). O
sistema de Newton é `(I − Φ′)δx − (∂Φ/∂T)δT = Φ_T(x₀) − x₀`, GMRES nas
incógnitas escaladas (`x_i/tol_i`, `T/(RELTOL·T)`); T fica no slot de
OSCNODE para o operador seguir identidade + posto baixo (com T no
fim, a identidade vira um deslocamento cíclico e o GMRES perde a
convergência em poucos produtos). **∂Φ/∂T pelo replay esticado**
(previsto na D-0032): cada produto perturba x e T juntos num único
replay; T + δT refaz a grade congelada com todos os tempos escalados
por `(T+δT)/T` (TSTEP e TMAX também), então o mapa é suave em T como é
em x — todos os passos e coeficientes de integração escalam junto.
Salvaguardas: passo de Newton com |δT| > 20% de T é encolhido
inteiro; backtracking em x e T juntos (até 4 metades, como no
excitado). **Pelo menos um passo de Newton**: o critério de
descasamento sozinho admite um erro de período de ~`tol/|ẋ|` (o
anel "convergia" a 1,2e-4 do T discreto com resíduo 0,7); um passo
a partir do estimado corrige T quadraticamente e o `δT` final vai na
nota. Uma "órbita" convergida sem excursão em OSCNODE é o
equilíbrio (periódico para todo T) e é recusada.

**Saída e falha.** Plots idênticos aos do excitado, `t₀` = o
cruzamento (fases dos harmônicos referidas a `t₀`, não a 0). Nota
com `[resíduo/tolerância, T]` por iteração e a última correção de T.
Falha = `Error::PssConvergence { iterations, residuals, period }`,
`period = Some(último T)` (no excitado `None`; a mensagem só ganha o
"last period" no autônomo).

**Correção de fidelidade que veio junto — `.ic` com UIC.** O
`CKTic` do 3f5 (`cktic.c`) põe os valores de `.nodeset`/`.ic` no
CKTrhs antes do ponto UIC (que só troca rhs↔solução e carrega) e
chama `DEVsetic`: o `CAPgetic` copia a tensão do nó para todo
capacitor sem `IC=`. O Dune não fazia nenhum dos dois — um tanque LC
chutado por `.ic v(1)=0.5` + UIC partia do repouso (ngspice oscila).
Agora `Engine::ckt_ic` + `Device::set_ic`. Regressão
`corpus/regress/uic_ic.cir` (LC + RC com `ic=` que vence o `.ic`):
6,1e-15 contra o ngspice-47. Nenhum deck do corpus usava `.ic` +
UIC; o resto é byte-idêntico.

**`getic` dos semicondutores (issue #20).** Portados todos os
`DEVsetic` do 3f5 (`*itf.h`): `DIOgetic`, `BJTgetic`, `MOS1/2/3/6getic`
(`mos*ic.c`), `JFETgetic` (`jfetic.c`), `MESgetic`, `B1getic`,
`B2getic`, e o `BSIM3getic` do ngspice-47 (`b3getic.c`). Todos fazem
`if (!icXGiven) icX = rhs[a] − rhs[b]` sobre os nós EXTERNOS
(BJT: `VBE = b−e`, `VCE = c−e`; MOS/BSIM: `VBS = b−s`, `VDS = d−s`,
`VGS = g−s`; JFET/MES: `VDS = d−s`, `VGS = g−s`). Os bits `*Given`
são os do `*param.c`: `IC=a,b,c` dá os n primeiros (o `switch` cai de
`case n` a `case 1`); `IC=a` só o primeiro; os `icvds=`/`icvgs=` do
MESFET um a um. Esses bits vivem numa tabela interna do `Circuit`
preenchida pela elaboração do netlist; um spec tipado montado pela
API (tupla `ic` sem "dado/não dado") conta como tudo dado — a API
pública não mudou (trocar as tuplas por `Option` por componente
ficaria a decidir). **Pega do diodo:** o `DIO_IC` do `dioparam.c`
grava `DIOinitCond` sem nunca ligar `DIOinitCondGiven` — no 3f5 E no
ngspice-47 —, então sob UIC o `IC=` do diodo é SEMPRE substituído
pela diferença dos nós (0 sem `.ic`). Medido no ngspice: `d1 … ic=0.65`
e `d1 …` sem `ic` dão rawfiles idênticos sob UIC. O Dune segue (antes
honrava o `IC=`). Regressão `corpus/regress/uic_getic.cir` (BJT com
`IC=VBE` parcial, BJT sem IC, diodo com `IC=` sobrescrito, MOS1, todos
polarizados por `.ic` + UIC): margem 0,013 (FAIL a 19× sem o porte).

**Validação** (`crates/dune-core/tests/pss_osc.rs`; decks
`corpus/pss/osc_*.cir`, goldens por `tools/pss-golden.sh`, que ganhou
`*pss-ngspice-card:` — o cartão inteiro que o ngspice roda, pois o
FREQ dele é chute obrigatório — e `*tran-golden-vectors:`). Período
de referência = contador de frequência (cruzamentos ascendentes do
médio-de-faixa) sobre a janela salva de um `.tran` longo. Régua de
período: erro de fase do trapezoidal por período, `(2π/N)²/12`
relativo com N passos por período, somado sobre os dois cálculos
comparados. Default = POINTS 256, tolerâncias SPICE.

| Deck (OSCNODE) | T do `.pss` | vs `.tran` ngspice (régua) | vs `.tran` Dune no TMAX do PSS | iterações [resíduo] | `.pss` do ngspice-47 |
|---|---|---|---|---|---|
| `osc_vdp` LC + −G + diodos (v(1)) | 199,36609 µs | 3,0e-5 (1,3e-4) | 2,8e-7 | 2 [4,9e-3, 1,6e-7] | 4012,9 Hz: **−25%** |
| `osc_ring` anel 3 CMOS MOS1 (v(1)) | 585,8960 ps | 1,9e-4 (2,9e-4) | 5,8e-7 | 2 [1,9e-2, 2,6e-6] | 1,5756 GHz: **−8,3%** |
| `osc_phase` BJT + escada RC (v(c)) | 1,9157359 ms | 3,6e-5 (1,4e-4) | 2,0e-7 | 2 [1,7e-2, 1,9e-6] | 497,15 Hz: **−5,0%** |
| `osc_wien` Wien + opamp_ideal + diodos (v(out)) | 633,96148 µs | 8,9e-6 (8,3e-5) | 2,0e-7 | 2 [8,1e-4, 2,1e-7] | não converge (estimativa vaga até 5,3 kHz) |
| `osc_colpitts` (exemplo do ngspice) (v(3)) | 299,09398 ns | 1,6e-5 (8,7e-5) | 1,5e-6 | 2 [3,8, 2,3e-2] | 3,343602 MHz: 3,6e-5 do `.tran` dele, 5,1e-5 do Dune |

- O `.tran` do Dune no mesmo cartão bate o do ngspice a 4e-12 (anel)
  … 1,2e-6 (phase): a diferença PSS-vs-`.tran` é toda de malha (o
  `.tran` do cartão tem ~117–320 passos por período, o PSS 256); no
  mesmo TMAX = T/256 sobra 2e-7–1,5e-6.
- **Apertado** (RELTOL 1e-6, POINTS 1024) contra o `.tran` do Dune
  apertado com TMAX = T/1024: vdp 5,5e-9, Wien 5,2e-9 — os dois
  métodos convergem um sobre o outro.
- **Analítico (vdp):** `T0 = 2π√(LC)` = 198,6918 µs; o PSS fica
  3,35e-3 acima — o puxão dos harmônicos (os diodos grampeiam
  duro). Groszkowski é **exato** para tanque LC carregado só por
  não-linearidades sem memória (Σ k·Q_k = 0 → ω²/ω₀² =
  Σ|V_k|²/Σk²|V_k|²); com o espectro do próprio PSS (1024 pontos, 200
  harmônicos, apertado) dá 3,4e-6 do T do shooting.
- O `.pss` do ngspice, no território dele (oscilador livre), só
  acerta no Colpitts — e só com o cartão do `colpitt_osc_pss_ctrl.cir`
  (`1e6 50e-6`); com o do deck simples (`3.1e6 500e-6`) diverge para
  11 MHz. Nos outros quatro declara convergência em frequências
  5–25% erradas (vdp, anel, phase — conferido contra o `.tran` do
  próprio ngspice) ou não converge (Wien). Com chute 1 kHz no vdp
  (5× abaixo) o ngspice-47 não termina em 9 min.

**Custo** (períodos simulados; a estabilização roda em malha grossa):

| Deck | TSTAB | estabilização | refino | adaptativos + replays | `.tran` do golden |
|---|---|---|---|---|---|
| vdp | 4 ms | 20 T | 5,5 T | 2 + 3 | 200 T |
| anel | 100 ns | 171 T | 5,5 T | 2 + 4 | 341 T |
| phase | 100 ms | 52 T | 5,5 T | 2 + 4 | 104 T |
| Wien | 10 ms | 16 T | 5,5 T | 2 + 4 | 158 T |
| Colpitts | 500 µs | 1672 T | 5,5 T | 2 + 5 | 2006 T |

O shooting em si custa 5–7 períodos; o resto é o TSTAB que o deck
pede (o tempo físico de partida do oscilador — o Colpitts herdou os
500 µs do cartão do ngspice). Com o T estimado, o Newton não espera a
amplitude assentar.

**Pendências:** osciladores com fonte periódica que *não* é a
fundamental (VCO travado, injeção) seguem o caminho excitado — não
há opção para forçar o autônomo com fontes; o modo de fase quase
neutro do `osc_tline` converge devagar (×0,85/iteração, 16
corridas) e pede aceleração; `.pss` com `devnoise` ligado não
injeta nem recusa (decidir). ~~`Circuit::run_card` devolvendo o
`Pss`~~ e ~~espelhos de `notrnoise`/`devnoise` nos bindings~~:
feitos (2026-11-08, §Caminho único cartão → análise e
`docs/dune-sessao.md` §Opções nos bindings).

### Linhas de transmissão no `.pss` (issue #20)

**O problema.** O estado de uma linha é o seu histórico — as ondas que
saíram de cada porta, lidas um atraso `TD` depois (TRA, LC do LTRA) e,
nas perdas (RLC/RC), a convolução de núcleos de cauda `τ^{−3/2}` sobre
tudo o que passou desde o ponto de operação. O vetor do shooting
(`x₀` + bancos de estado) não o contém; até aqui o `.pss` recusava T/O
com `Unsupported`.

**Escolha: opção A (o histórico é a órbita), com uma fixação de DC.**
No regime periódico `x(t − τ) = x(t − τ + kT)`: o passado inteiro é a
órbita repetida. A incógnita do Newton continua `x₀`; o histórico vira
um ponto fixo resolvido junto (Picard). A opção B (estados do
`ltrafit` como incógnitas extras) não foi adotada: o atraso puro `TD`
exige a órbita de qualquer jeito (opção A), os estados recursivos de
um período passado infinito saem em **forma fechada** da própria
órbita (abaixo) — pô-los no vetor só acrescentaria incógnitas
redundantes — e o modo que de fato não converge (o DC das perdas,
abaixo) é singular para o Newton também (`1 − λ ≈ 1e-5`, enterrado no
ruído das diferenças), então incluí-lo no GMRES não resolveria.

- **O passado sintetizado** (`matrix::PeriodicPast`): fases `u ∈ (0, 1)`
  e linhas de solução de um período da órbita; cada fronteira de
  período é o próprio `x₀` (o fechamento da onda termina no estado de
  que o intervalo parte, e o produto `Φ′v` vê `x₀` também nas
  fronteiras do passado). Chega aos dispositivos pelo
  `AcceptCtx::periodic` do primeiro accept do intervalo (sem gancho
  novo no trait `Device`); as linhas montam ali o histórico de períodos
  inteiros até o atraso mais velho que um load pode pedir: a tabela
  `TRAdelays` (TRA; `MODEINITTRAN` passa a ler a tabela em vez de
  semear o operating point), os vetores `times/v/i` do LTRA (LC: só
  atraso; RLC: `TD`; RC com `ltrafit`: a janela exata `RCl²/8`). Os
  breakpoints que a regra de inclinação (`TRAaccept`/`LTRAaccept`)
  teria plantado um atraso depois das quinas do passado são plantados
  onde caem dentro do intervalo.
- **Estados recursivos (`ltrafit`) em forma fechada.** Com o desvio
  `d = w − w̄` (a média da órbita, não o operating point — não há),
  um período de `d` leva o estado de zero a `P_j`; periodicidade
  `S = e^{−x_j T}S + P_j` dá `S_j = P_j/(1 − e^{−x_j T})` — a soma
  geométrica exata sobre todos os períodos anteriores
  (`FitState::periodic_start`). O horizonte do ajuste vai a
  `max(TSTOP, 1000·T)`; além dele `d` tem média nula e o que o ajuste
  perde se cancela a cada período.
- **RLC/RC exigem `.options ltrafit`** (erro claro sem ela; decisão de
  desenho, pausa): o caminho exato soma um histórico que começa num
  operating point; o passado periódico é infinito, e só o modelo
  recursivo o soma (em forma fechada). LC, RG e TRA não precisam.
- **Fixação de DC (o achado que custou caro).** Com `G = 0` a forma
  característica perde a queda DC da linha: `Y0(0) = 0`, então em
  `s = 0` as duas equações de ramo viram `i1 + i2 = 0`, e
  `v1 − v2 = R·LEN·i1` só aparece na ordem `√s` — no tempo, as caudas
  integradas sobre um histórico que *começa* num operating point (cujos
  carimbos DC são esse resistor). Um passado sempre periódico não tem
  começo: a queda média é um modo neutro que a órbita só copia. Medido
  (RLC sozinho, apertado): a resistência DC aparente derivando
  1,85e-6 A por iteração para longe da verdade, constante, com e sem
  Newton, independente do horizonte do ajuste (10, 1000, 1e5 T). O
  passado é então **fixado na relação DC exata**: as tensões médias
  deslocadas simetricamente, a corrente média mantida, de modo que a
  queda média seja `R·LEN` vezes a corrente média. Num ponto fixo o
  passado é a órbita, então a própria órbita cumpre a relação (o
  deslocamento só é não nulo enquanto itera). É a relação de dois
  portos em `s = 0`, válida para qualquer circuito em volta.
  **O mesmo modo neutro afeta o `.tran` longo** (ngspice e Dune, exato
  ou recursivo): o DC deriva como `√t` (abaixo).
- **Grade congelada.** Uma corrida adaptativa (passado: o último
  período da estabilização) fixa a grade; nela, cada iteração refaz o
  intervalo (`Engine::tran_replay_rows`) lendo a órbita anterior como
  passado (Picard) e, enquanto o descasamento de período passa da
  tolerância, dá um passo de Newton em `x₀` com esse passado congelado
  (o GMRES do excitado). Convergiu na grade quando o descasamento de
  período **e** o de histórico (`max |x_i(t) − passado_i(t)|/tol_i`
  nas incógnitas de porta das linhas, nos mesmos instantes — exato,
  sem interpolar entre grades) cabem na tolerância. A grade é então
  renovada **uma vez** (adaptativa a partir do resultado, que escolhe
  uma grade para a órbita convergida) e a iteração converge de novo
  nela. Sem congelar, o histórico não converge: entre duas grades as
  linhas não se reproduzem na tolerância SPICE — a regra de inclinação
  planta o breakpoint de eco pelo **sinal** de uma inclinação
  (`|d1−d2| ≥ max + 1` só dispara em troca de sinal; com `d2` ~0
  depende do ruído), então duas corridas põem uma quina atrasada até um
  passo de distância, dezenas de tolerâncias na quina (medido: 10–60,
  sem convergir em 50 iterações); e o erro da convolução do LTRA é da
  grade, não do RELTOL (não há LTE no LTRA). O descasamento da corrida
  de renovação é essa discretização entre grades e vai na nota, não é
  iterado.
- **Autônomo.** Mesmo esquema com o passo de Newton `(x₀, T)` da
  D-0035; a grade congelada e o passado esticam com `T` (ambos são
  funções da fase); pelo menos um passo de Newton. A continuação de
  refino de 5,5 períodos e o shooting leem o último período estimado
  da estabilização como passado.

**Validação** (`crates/dune-core/tests/pss_lines.rs`; decks e goldens
em `corpus/pss/`, `tools/pss-golden.sh`). Régua D-0010
`2·TRTOL·RELTOL·S` com o tubo de ±1 passo da D-0011 (as linhas põem
quinas que duas grades colocam até um passo de distância; a distância
pontual vai junto na nota do teste).

| Deck | Iterações (custo) | vs `.tran` ngspice (tubo / régua) | vs `.tran` Dune no TMAX do PSS (T/256) |
|---|---|---|---|
| `pss_tra` (TRA TD=7 ns < T e TD=55 ns > T, grampo a diodo, pulsos 25 MHz) | 10 (2 adaptativas + 8 replays, nenhum Newton: o descasamento de período já cabe) | v(2) 2,6e-4 / 2,6e-2; v(3) 2,7e-4 / 3,1e-2; v(4) 2,6e-3 / 1,7e-2; v(5) 6,1e-5 / 1,0e-2 | v(3) 7,0e-4; v(5) 9,2e-4 |
| `pss_ltra` (LTRA RLC + RC com `ltrafit`, LC atrás de grampo) | 4 (2 + 2) | v(2) 3,7e-3 / 5,2e-2; v(3) 1,4e-2 / 6,0e-2; v(4) 3,0e-5; v(5) 1,3e-4; v(6) 7,8e-3 / 3,6e-2; v(7) 4,2e-4 / 1,1e-2 | v(3) 1,4e-2 (com e sem `ltrafit` no `.tran`: 1,44e-2 / 1,43e-2); v(5) 3,3e-4; v(7) 7,1e-4 |
| `osc_tline` (inversor CMOS fechado por TRA TD=1 ns, autônomo) | 16 (2 + 22 replays, GMRES 3) | T: 6,4e-5 do ngspice a 256 pontos; 1024 pontos: 3,3e-6 (régua de fase 6,0e-6) | T: 2,0e-5 |

- **Apertado** (RELTOL 1e-6, 1024 pontos) contra o `.tran` do Dune
  apertado (T/1024, 100 períodos): `pss_tra` v(3) 3,7e-5, v(5) 1,3e-5;
  `pss_ltra` v(3) 5,0e-4, v(5) 1,0e-4, v(7) 2,5e-6 (parte disso é a
  deriva de DC do `.tran`, abaixo).
- **Ponto a ponto contra o apertado** (`pss_ltra`, ambos a T/256): o PSS
  fica a 4,0e-2 em v(3) e 2,9e-2 em v(5) (quinas), o `.tran` a
  7,4e-2/3,3e-2; no patamar o PSS fica a ~1e-5 e o `.tran` a ~6e-4 —
  a deriva de DC.
- **DC (superior ao `.tran`).** A média da resposta periódica de rede
  linear é o ganho DC vezes a média do estímulo, exatamente:
  `2,1 V · 100/114,8 = 1,829268 V` nas duas pontas distantes. PSS:
  v(3) +7,8e-5, v(5) +1,7e-5. `.tran` do ngspice (convolução exata) após
  50 períodos: v(3) −6,3e-4, v(5) +1,3e-4. `.tran` do Dune com
  `ltrafit`: v(3) −5,8e-4 (25 períodos), −1,1e-3 (100), −2,2e-3 (400),
  −3,0e-3 (800) — cresce como `√t` (×1,4 por dobra); o exato do Dune
  segue junto (−5,0e-4/−7,0e-4/−9,7e-4 a 25/50/100). É o mesmo modo
  neutro da queda DC, empurrado pela discretização a cada passo; o
  `.tran` longo **não** converge para o regime permanente nesse modo.
- **Fidelidade do `.tran` com linha**: no cartão do golden do
  `osc_tline` o `.tran` do Dune bate o período do ngspice a 2,7e-9.
- **`.pss` sem linhas bit-idêntico**: as 18 provas de
  `tests/pss.rs`/`tests/pss_osc.rs` e rawfile + stdout dos 126 decks
  do corpus (`corpus/**/*.cir`, inclusive os `.pss`) idênticos byte a
  byte ao main (os 3 decks novos eram recusados lá).

**Convergência.** O Picard da TRA contrai pelo produto das reflexões
por volta (pss_tra: ×0,2–0,5 por iteração). No autônomo o histórico
tem um modo quase neutro de fase (deslocar o passado no tempo; a
condição de fase prende só `x₀[osc]`): ×0,85 por iteração na segunda
grade do `osc_tline` — converge, mas é o custo dominante (22 replays).
Sem aceleração (Anderson sobre o passado foi descartado para o DC:
o mesmo `1 − λ ≈ 1e-5` deixa a secante no ruído).

**Pendências:** linha num deck com `TD` de muitos períodos custa
`TD/T` cópias do passado (limite `MAX_PAST_SAMPLES`, erro claro);
acelerar o modo de fase do autônomo com linha; bindings (`spice.pss`,
`spice_pss`) ainda documentam linhas como recusadas (outro território).

## Ruído transiente — `TRNOISE`/`TRRANDOM` (issue #13, parte 2)

Sem contrato 3f5: a gramática e a semântica são as do **ngspice-47**
(manual §4.1.7–4.1.8; `vsrcload.c`, `vsrcacct.c`, `vsrcpar.c`,
`frontend/trannoise/1-f-code.c`; a fonte I espelha). Os geradores
aleatórios são do Dune. Módulo `trnoise.rs`; a realização mora no
dispositivo da fonte (`NoiseWave`), a onda crua em `Wave::TrNoise`/
`Wave::TrRandom` (args como no cartão, igual às outras ondas).

**Gramática** (fontes V e I, com ou sem parênteses):

- `TRNOISE(NA NT <NALPHA NAMP <RTSAM RTSCAPT RTSEMT>>)` — 2 a 7
  argumentos. Amostras `x₀ = 0, xₖ = NA·gₖ + (yₖ − y₀)` na grade
  `k·NT` (`gₖ` gaussiana padrão; `y` a sequência 1/f^NALPHA de Kasdin
  com ruído branco motor de rms NAMP, `hₖ = hₖ₋₁(α/2 + k − 1)/k`,
  `⌊TSTOP/NT⌋ + 10` amostras); o valor é **linear entre amostras**
  (`V1 + (V2−V1)·frac` do `VSRCload` — não é degrau). `NT = 0`: sem
  branco/1-f. NAMP só vale com NALPHA ≠ 0 e o 1/f só existe com
  NALPHA > 0 e NAMP > 0 (`trnoise_state_gen`). RTS (só com
  RTSAM > 0): soma RTSAM com a armadilha cheia — captura após tempo
  exponencial de média RTSCAPT, emissão após um de média RTSEMT;
  alto em `[capᵢ, emᵢ]` (o `VSRCload` testa `time >= RTScapTime` e a
  emissão é aplicada no accept após `emᵢ`).
- `TRRANDOM(TYPE TS <TD <PARAM1 <PARAM2>>>)` — 2 a 5 argumentos;
  `PARAM2` até TD, depois um sorteio novo em cada intervalo
  `(TD+(k−1)TS, TD+kTS]`: tipo 1 uniforme `PARAM1·[−1,1) + PARAM2`, 2
  gaussiana (σ PARAM1, média PARAM2), 3 exponencial (média PARAM1 +
  PARAM2), 4 poisson (λ PARAM1 + PARAM2, corte em 1000 como o
  `poisson` do `randnumb.c`). PARAM1 default 1, PARAM2 0.
- Ambos **somam o valor DC** quando dado (o ngspice soma; as outras
  ondas não) e valem 0 (TRRANDOM: PARAM2) em t = 0, logo o ponto de
  operação e toda análise DC/AC são sem ruído. Argumentos fora de
  domínio são erro de build (ver KNOWN_DIFFS); aridade errada é erro
  com span no parser.
- `alter`: `trnoise`/`trrandom` entram na lista de parâmetros da
  fonte (`Circuit::alter`, mesmo formato de lista das outras ondas).

**Passo.** O ngspice não mexe no TMAX: o `VSRCaccept` planta um
breakpoint em cada ponto da grade (`break_time += TS`) e em cada
borda do RTS; o integrador sempre pisa nas quinas e, entre elas, a
fonte é linear (TRNOISE) ou constante (TRRANDOM). O Dune faz o mesmo
pelo hook `accept` da fonte (próximo ponto `(⌊(t+minBreak)/NT⌋+1)·NT`,
multiplicação em vez de soma acumulada — o ngspice deriva ~1e-6·NT
em 10⁵ passos). Diferença: uma grade que só carrega zeros (`NA = 0`
sem 1/f) não planta breakpoints. Cada breakpoint custa ~4–5 passos
(o `DCtran` recomeça em 0,1 do vão), então o orçamento automático
(D-0022) vira `max(automático, 50·⌈TSTOP/espaçamento⌉)` quando há
fonte de ruído (`Circuit::noise_tran_options`; o engine não muda),
e uma grade que sozinha estoura o guarda de memória, ou um 1/f de
mais de 2²³ amostras, é recusada. Toda corrida com ruído deixa a
nota `transient noise: N source(s), seed S (stream version 1)`.

**Semente e PRNG.** `.options seed=N` — **a chave do próprio
ngspice-47** (`inp.c` lê `option seed=val|random` e põe em
`rndseed`; o `set rndseed=N` do front-end é o mesmo número).
`Options::seed: u64`, default **1** (o `rndseed` de partida do
`main.c` do ngspice). `seed=random` (relógio) e valores ≤ 0 são
recusados com aviso, a semente fica. O gerador é **contado**
(counter-based): todo número é função pura de `(seed, nome da fonte,
fluxo, índice)` — chave = SplitMix64 encadeado sobre a semente, o
FNV-1a 64 do nome em minúsculas e o id do fluxo (branco, 1/f, RTS,
TRRANDOM); o item `k` usa o `k`-ésimo valor do SplitMix64 da chave
como semente de um fluxo local (Steele–Lea–Flood 2014: incremento
`0x9E3779B97F4A7C15`, mistura `0xBF58476D1CE4E5B9`/
`0x94D049BB133111EB`; BigCrush). Gaussiana pelo método polar de
Marsaglia (um da dupla), exponencial `−ln U`, poisson por chegadas.
Só aritmética inteira, IEEE e o `libm` em Rust puro (já dependência:
sem libm da plataforma) → **os mesmos bits em toda plataforma**,
wasm32 incluído; pinados no teste `streams_are_pinned`
(`STREAM_VERSION = 1`, a subir com os goldens se algo mudar os
bits). Consequências (todas melhores que o ngspice, cujas fontes
partilham UM fluxo global semeado por `rand()` da libc): a
realização de uma fonte não muda quando outras fontes entram, saem
ou trocam de ordem, nem quando o controle de passo rejeita e refaz
um ponto; mesma semente ⇒ mesmos bits sempre.

**1/f.** Kasdin pede convolução **linear** (FFT com zero-padding até
2N); é o que o Dune faz (FFT radix-2 própria, twiddles do `libm`). O
`f_alpha` do ngspice faz produto circular — e no build com FFTW (o
do Homebrew: imprime `100010 1/f noise values`, comprimento não
potência de 2) o produto espectral indexa os vetores intercalados
`re,im` por `i, i+1` em vez de `2i, 2i+1` (`1-f-code.c`,
`out[i][0] = hfa[i]*wfa[i] - hfa[i+1]*wfa[i+1]`): o espectro sai
deformado. Medido abaixo.

**Validação** (`crates/dune-core/src/trnoise.rs` testes unitários;
`crates/dune-core/tests/trnoise.rs` circuito; réguas = 5σ do erro
amostral, sementes fixas):

| Caso | Referência | Resultado |
|---|---|---|
| 10⁶ amostras brancas (unitário) | média 0, var NA², curtose 3, correlação lag-1 0 | dentro de 5σ (σ_var = √(2/N) = 0,14%) |
| branco sobre R, 2·10⁵ amostras, seed 42 | var NA², massa ±1σ/±2σ, ⟨v²⟩ₜ = ⅔NA² (interpolação linear; degrau daria 1) | var/NA² − 1 = +0,41% (σ 0,32%); P(±1σ) 0,68178 (0,68269); P(±2σ) 0,95420 (0,95450); ⟨v²⟩ₜ/NA² 0,66976 |
| RC (τ = 10·NT) sobre branco, 4·10⁵ amostras | forma fechada do segurador de 1ª ordem: `NA²[b₁² + (ab₁+b₀)²/(1−a²)]`, `a = e^(−NT/τ)`, `b₁ = 1−τ(1−a)/NT`, `b₀ = 1−a−b₁` (= 0,047582; o limite NA²·NT/2τ dá 0,05) | −0,30% (σ 0,71%) |
| 1/f, α = 0,5/1/1,5, 16 realizações de 2¹⁶ (unitário) | inclinação −α do periodograma (Hann) em f ∈ [10⁻³, 10⁻¹] | −0,4989 / −0,9978 / −1,4966 (régua ±0,08) |
| convolução do 1/f | soma direta `Σ hⱼwₖ₋ⱼ` | 1e-12 |
| RTS, 2·10⁴ ciclos | permanências médias RTSCAPT/RTSEMT | dentro de 5σ (σ = 0,71%) |
| TRRANDOM 1–4, 10⁵ sorteios | médias/variâncias das 4 leis | dentro de 5σ |
| reprodutibilidade | mesma seed ⇒ mesmos bits; seed 2 ≠ seed 1 (só t=0 coincide); `.options seed=2` = campo; fonte nova não altera as outras | ok |
| `.pss` com ruído | erro claro | `Error::Unsupported { name, "a transient-noise source (TRNOISE/TRRANDOM) in .pss — …" }` |

**Contra o ngspice-47** (mesmos decks, estatísticas — os fluxos são
outros números, nada amostra a amostra; scripts ad hoc, não no CI):

| Deck | Grandeza | ngspice-47 | Dune | Referência |
|---|---|---|---|---|
| branco NA=1, NT=1µ, 2·10⁵ amostras | var na grade | 1,0129 | 0,9931 | 1 (σ 0,32%) |
| idem | DSP média nas bandas [10⁻³,10⁻²) [10⁻²,10⁻¹) [0,1;0,3) [0,3;0,5) ciclos/amostra | 0,961 1,016 1,002 1,023 | 0,984 0,976 0,986 0,987 | 1 |
| RC τ=10·NT, mesmo deck | var de v(2) | 0,04796 (+0,8%) | 0,04679 (−1,7%) | 0,047582 (σ 1,0%) |
| 1/f α=1 NAMP=1, 10⁵ amostras, seeds 3/4/9 | inclinação do periodograma | −1,06 / −0,96 / −0,96 | −1,01 / −1,04 / −0,90 | −1 |
| idem | DSP em f = 0,01 | 24,8 / 26,2 / 36,9 | 15,6 / 15,1 / 19,0 | Kasdin 1/(2 sin πf) = 15,92 |
| RTS 1 V, 20µ/30µ, 100 ms | fração alta; transições | 0,5970; 3995 | 0,5967; 3962 | 0,6; ~4000 |
| TRRANDOM gauss(σ 0,5, μ 1) | média; var | 0,9922; 0,2481 | 0,9966; 0,2514 | 1; 0,25 |
| TRRANDOM poisson(λ 3) | média; var | 2,992; 2,987 | 3,003; 3,001 | 3; 3 |

Branco, RC, RTS e TRRANDOM batem dentro do erro amostral nos dois. O
1/f do ngspice sai 1,6–2,3× acima da forma fechada para o mesmo
NAMP; uma reprodução em numpy do produto do `f_alpha` com o
deslize de índice dá 30,5 (média de 6 realizações) e a do produto
circular correto dá 14,3–15,5 — a convolução circular sozinha não
explica, o índice sim. O Dune segue a forma fechada (KNOWN_DIFFS).
Densidade de passos: o deck branco/RC dá a mesma contagem de pontos
nos dois (1 002 697 vs 1 002 699); no deck com 1/f+RTS+TRRANDOM o
ngspice dá 954 k pontos e o Dune 520 k — **investigado (issue #20
item 4): artefato do ngspice, não fidelidade a corrigir.** O
`VSRCaccept` de cada fonte acumula o próprio breakpoint e desconta
`CKTminBreak` a cada disparo (`break_time += TS; CKTsetBreak(…);
break_time -= CKTminBreak`), então o k-ésimo ponto da grade cai
`(k−1)·minBreak` adiantado. Grades de TS diferentes adiantam em
ritmos diferentes: num deck com `TRNOISE` 1/f a 1 µs e `TRRANDOM` a
10 µs, o ponto de 9 ms chega como `9 ms − 9,0e-13` (9000 descontos)
de um e `9 ms − 9,0e-14` (900) do outro — medido nos instantes do
rawfile, batendo com `(k−1)·1e-16` —, e o `CKTminBreak = 10·CKTdelmin`
do build com XSPICE (o do Homebrew: `1e-10·TMAX` = 1e-16 aqui; sem
XSPICE seria `5e-5·TMAX`) não os funde. Cada par vira um
breakpoint-fiapo, o `DCtran` recomeça em `0,1·vão` e sobe dobrando:
~20 passos extras por coincidência (16 539 passos < 1e-10 s em 77 k
pontos no deck de 10 ms; o Dune tem 0). Medido em decks de 100 ms
(1/f 1 µ + RTS + dois `TRRANDOM`): TS 10 µ → ngspice 734 949, Dune
520 769; TS 2/5 µ → 1 899 402 vs 520 769; com todas as grades no
mesmo TS (o deck da tabela acima, com uma fonte por tipo) as
contagens batem (520 738 vs 520 769), como no branco/RC. O Dune
planta `(k+1)·TS` por multiplicação — os pontos coincidem bit a bit
e o `CKTsetBreak` os funde — e a forma de onda é a mesma (o valor da
fonte vem de `t/TS`, não do breakpoint). Eficiência legítima;
KNOWN_DIFFS registra.

**Sondas** (`corpus/regress/trnoise_white.cir`, `trnoise_1f_rts.cir`,
`trrandom.cir`): golden do **próprio Dune** em
`corpus/regress/golden/<stem>.dune.raw` (versionados — negação no
`.gitignore`), comparados a 1e-12 relativo (os fluxos são bit-exatos;
o LU em volta pode variar em ulps entre builds SIMD); regenerar com
`DUNE_BLESS=1 cargo test -p dune-core --test trnoise`. Estão no
`corpus/SKIP` (fora do `--ci` do oráculo: não há golden ngspice
comparável).

**`.options notrnoise`** (issue #20; a variável `notrnoise` do
ngspice-47, lida no `trnoise_state_gen` do `1-f-code.c`): todo
`TRNOISE` é validado e depois perde TODOS os parâmetros de ruído
(`NA = TS = NALPHA = NAMP = RTSAM = RTSCAPT = RTSEMT = 0`) — vale só o
`DC`, sem grade, sem breakpoints —, para comparar com/sem ruído sem
editar o deck. **O `TRRANDOM` não é afetado**, como no ngspice (o
gerador dele nunca lê a variável; medido no ngspice-47: com
`.options notrnoise` o `TRNOISE` fica em DC e o `TRRANDOM` segue
sorteando a cada TS). `Options::notrnoise: bool` (flag ou `=0|1`); a
nota da corrida conta as fontes ativas e diz `notrnoise: N TRNOISE
source(s) silenced`. Ortogonal ao ruído de dispositivo (abaixo).
Teste `notrnoise_silences_trnoise_only` (`tests/trnoise.rs`).
Espelho nos bindings (listas de override Py/R/wasm): pendente — outro
território.

**Pendências**: `.pss` com ruído (recusado nesta fase).

## Ruído de dispositivo no tempo (issue #20 item 3) — além do ngspice

Nem o 3f5 nem o ngspice-47 têm (o ngspice só as fontes independentes
`TRNOISE`/`TRRANDOM`). **Opção proposta (pausa de naming):**
`.options devnoise` (flag ou `=0|1`; `Options::dev_noise`) e
`.options devnt=NT` (`Options::dev_noise_step`, default = TSTEP do
cartão). Default desligado: sem a opção, tudo byte-idêntico. Módulo
`devnoise.rs`; o injetor vive no `Engine` (`set_dev_noise`, só o
`Circuit::tran` liga — o `.pss` não injeta).

**Os geradores não são descritos duas vezes.** Cada dispositivo já
sabe os seus para o `.noise` (`noise_dens`); o injetor chama esse
mesmo código com um adjunto nulo e os coletores do `NoiseCtx`:
`sp_sink` (já existente, do `.sp`) devolve `(n1, n2, densidade)` de
todo térmico/shot (`4kT·g`, `2q|I|`), e o novo `flicker_sink`
devolve `(n1, n2, coef, EF)` de todo flicker (`NoiseCtx::flicker`,
para onde migraram os fatores `KF·…/f^EF` de diodo, BJT, JFET/MES,
MOS1/2/3 e BSIM3 `noiMod` 1/4/5 — o `.noise` sai igual a menos de
ulps). Cobertura = a do `.noise`: R, diodo (RS, shot, flicker), BJT
(RC/RB/RE, Ic/Ib shot, flicker em Ib), MOS1/2/3 (RD/RS, canal ⅔gm,
flicker `KF·gm²/(f^AF·W·L·Cox)`), JFET/MES, chaves, BSIM3 (térmicos;
flicker só nos `noiMod` 1/4/5 — o 2/3/6 não é lei de potência:
nota `no transient form`), BSIM1/2 (térmicos + flicker
`KF·|Id|^AF/f`, desde 2026-11-08); MOS6 segue silencioso, como no
`.noise`.

**Discretização.** Uma grade `k·NT` para todos, breakpoint em cada
ponto (como o `TRNOISE`), corrente linear entre amostras, amostra 0
nula (ponto de operação sem ruído). Branco de densidade unilateral
`S`: amostras `σ·gₖ`, `σ² = S/(2NT)` (a sequência tem densidade
`2σ²NT = S` até `1/(2NT)`; a interpolação linear molda por
`sinc⁴(f·NT)`). Flicker `coef/f^α`: sequência de Kasdin unitária
(`yₖ − y₀`, a mesma `kasdin` do `TRNOISE`, α = EF) escalada por `√Q`,
`Q = coef·(2π·NT)^α/(2·NT)` (densidade de Kasdin `2·NT·Q/(2 sin πfNT)^α
≈ coef/f^α`; α = 1 → `Q = π·coef`). Injeção no RHS depois de todos
os loads (`rhs[n1] −= i, rhs[n2] += i`), só em `MODETRAN`.

**Aproximação de polarização (documentada).** As densidades
acompanham o circuito: reavaliadas em cada ponto da grade a partir da
solução aceita ali (estados `0`), e a amostra `k+1` usa a amplitude
do ponto `k` — corrente contínua, ruído de um intervalo fixado pela
polarização no início dele (atraso de um NT; um shot cuja corrente
muda ΔI num NT erra `ΔI/2I` na variância daquele intervalo). Para o
flicker (memória longa) o modelo é o estacionário modulado
`√Q(t)·y(t)`. Pontos aceitos entre os da grade não mexem nas
amplitudes, então elas não dependem de onde o controle de passo
pisou.

**Fluxos.** O PRNG contado da D-0033: gerador `j` do dispositivo
`nome` usa `stream_key(seed, nome, 0x100 + j)` (branco) ou
`0x10000 + j` (flicker), amostra `k` = `Stream::at(chave, k)`. Mesma
seed ⇒ mesmos bits; dispositivo ou fonte que entra/sai/troca de ordem
não muda os outros; rejeição de passo não muda nada. A nota da
corrida: `device noise: G generator(s) on D device(s), devnt …, seed
S (stream version 1)`. Orçamento de passos: a grade entra no
`noise_tran_options` como mais uma grade de ruído (50 passos por
ponto; recusa se estourar o guarda de memória).

**Validação** (`crates/dune-core/tests/devnoise.rs`, réguas 5σ do
erro amostral medido — σ das médias de banda vem do espalhamento
entre segmentos do Welch, sem supor correlação entre bins):

| Caso | Referência | Resultado |
|---|---|---|
| R = 10 kΩ sozinho, 2¹⁷ amostras, NT = 1 ns | variância na grade `2kTR/NT`; DSP `4kTR`; variância na banda [0,1; 0,2]/NT = `4kTR·Δf` | 0,99661 (σ 0,0039); 0,99477 (σ 0,0058); 0,98868 (σ 0,0116) |
| **RC: R = 1 kΩ, C = 1 nF, NT = τ/50** | **kT/C = 4,1439e-12 V² (2,036 µV rms)**; valor exato da grade (retentor de 1ª ordem) = 0,99007·kT/C | 4·10⁵ amostras, seed 3: 0,96767·kT/C (σ 1,6%; FOH 0,97738). 4·10⁶ (`DEVNOISE_LONG=10`): 0,98024·kT/C, FOH 0,99008 (σ 0,50%). 2·10⁶ ad hoc, seeds 4/9: FOH 0,99884 / 1,00200 (σ 0,71%) |
| diodo 1 mA (fonte I ideal, sem C) | shot `2qI·r_d²`, `r_d` = 25,864 Ω do op | DSP 1,00258 (σ 0,0058); o `.noise` do mesmo deck dá 0,999995 |
| diodo 1 mA, KF = 1e-10, AF = 1, 2¹⁸ amostras | inclinação −1; nível = lei discreta de Kasdin `r_d²·[2qI + KF·I·π·2NT/(2 sin πfNT)]` | −0,9956; 0,99854 (σ 0,0094) |
| emissor comum (RB/RC/RE, Ic/Ib shot, flicker), 2¹⁸ amostras a 100 ns | `.noise` do mesmo deck em [10 k, 100 k] Hz | 1,0285 (σ 0,029); 3× mais longo: 1,0155 (σ 0,019) |
| fonte comum MOS1 (canal ⅔gm, RD/RS, flicker) | idem | 1,0273 (σ 0,034); 3×: 1,0058 (σ 0,018) |
| reprodutibilidade | mesma seed = mesmos bits; seed 2 ≠ (só t = 0); sem a opção = zero; R1 sozinho igual com diodo, R, TRNOISE entrando | ok |

**Sondas** (`corpus/regress/devnoise_rc.cir`, `devnoise_diode.cir`,
`devnoise_bjt.cir`): golden do próprio Dune
(`corpus/regress/golden/<stem>.dune.raw`, 1e-12 relativo), no
`corpus/SKIP`; regenerar com `DUNE_BLESS=1 cargo test -p dune-core
--test devnoise`.

**Pendências:** ~~espelho nos bindings~~ (feito: `devnoise`/`devnt`
nas listas de override de Py/R; o wasm já passava qualquer cartão —
`docs/dune-sessao.md` §Opções nos bindings);
`.pss` com `devnoise` não injeta (o `.pss` não passa pelo
`Circuit::tran`; recusar lá é do dono do `pss.rs`); flicker do BSIM3
`noiMod` 2/3/6; correlação entre geradores (nenhum dispositivo do
3f5/ngspice a tem no `.noise`, então não há o que portar).
## Load paralelo de dispositivos (issue #9, alavanca 1)

`Options::load_threads` (API, não `.options`; feature nativa
`parallel`, a mesma do `sweep_parallel`; ignorado sem ela e no
wasm). `1` (default) = o `CKTload` sequencial de sempre; `0` = um
thread por núcleo; `n > 1` = `n` workers. Números e limiar em
`docs/perf.md` — só compensa com centenas de dispositivos caros e o
load dominando a LU.

**Desenho.** O que impede somar em paralelo não é a avaliação dos
dispositivos (independente), é a soma em ponto flutuante: uma posição
da matriz carimbada por vários dispositivos vale `((0 + a) + b) + c`
na ordem dos dispositivos, e qualquer outro agrupamento (somas
parciais por worker reduzidas no fim) muda ulps. Então os workers não
somam:

1. A lista de dispositivos é cortada em **chunks contíguos na ordem
   dos dispositivos** (peso `1 + nº de estados` por dispositivo,
   `4 × threads` chunks; com chunks ≥ dispositivos, um por chunk).
2. Cada worker avalia o seu chunk num `StampLog` próprio: a lista de
   `(posição, valor)` dos carimbos de matriz e de RHS, na ordem em que
   os dispositivos do chunk os fizeram — sem somar nada.
3. A **redução** roda na thread chamadora: reaplica os logs chunk a
   chunk, em ordem de chunk, `values[k] += v` carimbo a carimbo.

Isso executa exatamente as mesmas somas do laço sequencial, na mesma
ordem por posição — o resultado é **bit-idêntico** para qualquer
número de threads, qualquer particionamento e qualquer escalonamento
(a ordem de término dos workers não entra em lugar nenhum).

**Por que não há aliasing** (sem `unsafe`; o borrow checker prova as
três primeiras):

- *Campos do dispositivo*: cada worker recebe um `&mut [Box<dyn
  Device>]` disjunto (`split_at_mut`); `Device: Send`.
- *Estados*: os offsets são distribuídos em ordem de dispositivo
  (`StateAlloc`, um dispositivo por spec), então os estados de um
  chunk formam uma janela contígua de cada banco. `Built::state_ranges`
  guarda a janela de cada dispositivo; `States::split_views` corta os
  8 bancos nas fronteiras dos chunks e cada worker recebe um
  `StateView` só da sua janela. Um acesso fora dela é pânico de
  índice, nunca leitura de célula alheia.
- *Matriz/RHS*: o `LoadCtx` agora só expõe `add`/`add_rhs` (o
  `StampSink` é só-escrita); nenhum load lê o que outro carimbou. Os
  52 `ctx.rhs[i] ±= v` viraram `add_rhs(i, ±v)` — `a − v` e
  `a + (−v)` são a mesma operação IEEE.
- *`noncon`/`fault`*: OR e "o último dispositivo que acusou",
  mesclados em ordem de chunk — a semântica do laço sequencial.
- *Quem escapa da própria janela*: o indutor mútuo (`MUTload` soma no
  fluxo dos dois indutores). `Device::par_load()` é opt-in por tipo
  depois de auditado (22 tipos `true`, K fica `false`); um dispositivo
  não-paralelo **corta a lista**: carrega no lugar, na thread
  chamadora, entre o trecho paralelo anterior (já reaplicado) e o
  seguinte — mantém sua posição na ordem sequencial. O `load_late`
  (integração do indutor) segue sequencial depois de todos os loads,
  como antes.

**Verificação.** `crates/dune-core/tests/parload.rs` (no CI, em
`--release`): todo deck de `corpus/{spice3f5/examples, regress, basic,
pss, measure}` com 2, 3 e 64 threads contra 1, bit a bit em todos os
plots (op/dc/ac/tran/noise/pz/disto/sens/tf/sp/pss) — com 64 threads
cada dispositivo é o seu chunk, então a janela de estados de cada
worker é exatamente a de um dispositivo e qualquer tipo que tocasse
estado alheio entraria em pânico ali; mais uma cadeia BSIM3 de 12
inversores com um par K no meio da lista, em `.op`/`.dc`/`.ac`/`.tran`.
Teste de mutação manual: inverter a ordem do replay quebra a
bit-identidade na hora. Fora do teste: `ltra_3` (a cascata de
breakpoints do KNOWN_DIFFS custa minutos por rodada; usa só diodos e
LTRA, cobertos por `ltra_1`/`ltra_2`). O oráculo roda sequencial (default) e
segue em 76.

**Notas.** Um pool rayon por `Engine`, criado no primeiro load que
pedir `load_threads ≠ 1` (falha ao criar → load sequencial, mesmo
resultado). Dentro de `sweep_parallel` cada rodada criaria o seu pool:
não combine os dois com contagens altas (oversubscription). Laços
ainda sequenciais por dispositivo: `conv_test`, `trunc`, `accept`,
`load_ac` (o `.ac`/`.noise` por frequência é da alavanca 2).
