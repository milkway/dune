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

- **Orçamento de passos do `.tran`** (D-0022): nem o 3f5 nem o
  ngspice impõem o ITL5 que ambos parseiam (`cktsopt.c`:
  `case OPT_ITL5: break;`) — uma cascata de breakpoints do LTRA
  roda horas lá. O Dune conta toda tentativa de ponto (aceita ou
  rejeitada) contra `max(10 000, 50·⌈TSTOP/TMAX⌉)` (override:
  `Options.max_tran_steps`; corpus pica em 13 passos/vão) e falha
  com `Error::StepLimit`. Decks biestáveis no oráculo: flags
  `bistable`/`accept-error=` do `corpus/XFAIL` (formato de 5
  campos, ver docs/dune-oracle.md).

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

**Limitações/pendências**: linhas de transmissão recusadas
(`Error::Unsupported`: o estado é o histórico da linha — com
`ltrafit` a parte recursiva é finita, mas o atraso não; exigiria pôr
o histórico/estados no vetor do shooting); chaves com histerese
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

**Pendências:** LTRA/TRA no `.pss` (estado = histórico da linha,
recusado como no excitado); ruído de dispositivo no tempo (issue #20
itens 3–5); osciladores com fonte
periódica que *não* é a fundamental (VCO travado, injeção) seguem o
caminho excitado — não há opção para forçar o autônomo com fontes;
bindings dedicados (spice.pss/spice_pss) não expõem `periods` nem o
`period` da falha ainda (outro território).

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
ngspice dá 954 k pontos e o Dune 520 k (não investigado: as quinas
são as mesmas).

**Sondas** (`corpus/regress/trnoise_white.cir`, `trnoise_1f_rts.cir`,
`trrandom.cir`): golden do **próprio Dune** em
`corpus/regress/golden/<stem>.dune.raw` (versionados — negação no
`.gitignore`), comparados a 1e-12 relativo (os fluxos são bit-exatos;
o LU em volta pode variar em ulps entre builds SIMD); regenerar com
`DUNE_BLESS=1 cargo test -p dune-core --test trnoise`. Estão no
`corpus/SKIP` (fora do `--ci` do oráculo: não há golden ngspice
comparável).

**Pendências**: ruído transiente intrínseco dos dispositivos
(térmico/shot/flicker de R, diodo, BJT, MOS como fontes no tempo —
o ngspice só tem as fontes independentes; seria além dele);
`.pss` com ruído (recusado nesta fase); `notrnoise` do ngspice (não
portado — basta não ter a onda).
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
