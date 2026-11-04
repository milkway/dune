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
6. **NEWCONV**: o teste de corrente do diodo (`DIOconvTest`) só faz
   efeito com o bypass de dispositivos, que o Dune não implementa; a
   convergência fica com o `NIconvTest` + o flag de limitação do
   `pnjlim`, como no 3f5 efetivo.
7. **Fim de varredura `.dc` com epsilon** (ngspice): o 3f5 compara o
   valor acumulado cru com o stop e perde o último ponto quando o
   arredondamento passa de raspão; o ngspice (e o Dune) usam
   `> stop + DBL_EPSILON·1e3`. Passo zero/não-finito é erro (o 3f5
   entraria em laço infinito). Varreduras hostis têm teto de memória
   (~50 M valores) com erro limpo.

## Pendências (fases seguintes)

- `.tf`, ruído, `.pz`, `.disto`; `.dc` varrendo TEMP e parâmetro de
  modelo; `keepopinfo` (plot "AC Operating Point").
- Semicondutores R/C com modelo (w/l), `temp` por instância de R.
- BJT/MOS/JFET/MESFET, chaves, linhas T/O/U, fonte B (Fase 3).
- Bypass de dispositivos (`CKTbypass`) — otimização, não semântica.
