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
  Sem carga AC (o 3f5 não tem `mos6acld.c`). Mesmo andaime do MOS1.
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
  nomes ngspice `_thermal`/`_1overf`), BJT (rc/rb/re térmicos com
  `gx` do estado, shot em Ic/Ib, flicker em Ib, total), diodo (7
  vetores do ngspice — os 3 `*sw` sempre nomeados, zero sem RSW),
  MOS1/2/3 (térmico de canal 8kT·gm/3, flicker **NLEV=2 do
  ngspice** `KF·gm²/(f^AF·W·Leff·Cox)` — o 3f5 usava Id^AF/Cox²;
  fallback tox=1e-7 sem TOX; `NLEV`/`GDSNOI` não portados),
  JFET/MESFET (`fet3_noise` compartilhado) e chave S/W (um gerador
  térmico, integrado como no `swnoise.c` — dispositivo de gerador
  único integra o próprio total). BSIM1/2 ficam silenciosos (o 3f5
  não tem ruído deles; `b1noi.c`/`b2noi.c` são ngspice — deck
  `.noise` com BSIM divergiria do oráculo); MOS6 não tem em
  nenhum dos dois. Integração
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
  — detalhes no KNOWN_DIFFS. JFET/MES/BSIM1 têm código de
  distorção no C mas seguem não portados (erro estruturado);
  MOS6/BSIM2 não têm no C. Sondas: `mixdisto_hd`/`mixdisto_im`
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
  setup). **Diverge do oráculo onde o ngspice está quebrado** (AC
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
  corte de passo (o `E_INTERN` do C). Não portados (inalcançáveis
  ou sem deck): `LTRAlteCalculate` (exige `fullcontrol`, que
  nenhuma tabela aceita) e a compactação `trytocompact`. Exigiu
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
  Resta do BSIM3: NQS, ACM, `.disto`/`.sens`, b3getic/UIC.

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
  instanciado. Ruído via `fet3_noise` (pinado por `regress/fetnoise`); sem
  `.pz` ou `.disto`. Sondas: `regress/jfet_iv` (famílias I-V NJF/PJF/quente,
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

- Ruído de BSIM1/2 (`b1noi.c`/`b2noi.c`, só ngspice — hoje
  silenciosos, deck `.noise` com BSIM divergiria do oráculo);
  `NLEV`/`GDSNOI`; temp/dtemp de instância no `NevalSrc` (os
  térmicos usam a temperatura do circuito); `.pz` dos semicondutores restantes (BJT/
  MOS têm `pzLoad` no C, aqui ainda não); `.disto` de JFET/MES/BSIM1
  (`jfetdset.c`/`mesdset.c`/`b1dset.c` no C, não portados —
  MOS6/BSIM2 não têm no C); `.dc` varrendo TEMP e parâmetro de
  modelo; `keepopinfo` (plot "AC Operating Point").
- Semicondutores R/C com modelo (w/l), `temp` por instância de R.
- Linha U (URC) e fonte B (Fase 3, sem deck); JFET nível 2
  (Parker–Skellern) e MES 2/5/6 (MESA/HFET) são ngspice, fora do
  contrato 3f5.
- Oráculo: a coluna de escala não é comparada (a malha de
  frequência errada do acdec só apareceu como erro de variável) e
  o tubo do tran absorve um shift de malha inteiro (o tra_uic
  pré-fix passava com 0.68 V de erro pontual).
- Pz das chaves sem golden (só testes unitários); o ruído está
  pinado por `regress/fetnoise` (`onoise_s1`).
- LTRA: `trytocompact` (compactação do histórico) e o controle LTE
  (`LTRAlteCalculate` — inalcançável sem `fullcontrol`).
- Controlador de passo: cruzamento de `vds=0` num breakpoint com nó
  de ~1 fF mata o passo no BSIM1/2 (ver o caso-limite acima).
- Bypass de dispositivos (`CKTbypass`) — otimização, não semântica.
- Fallbacks restantes do op do ngspice: `gillespie_src` (rampa
  adaptativa de fontes) e o *transient op* (acha o `.op` por um
  pseudo-transiente — é o único que fecha um `.op` puro com BJT
  `off` atrás de fonte só-transiente; o caso `.tran` equivalente
  está coberto e regredido em `corpus/regress/offbjt.cir`).
