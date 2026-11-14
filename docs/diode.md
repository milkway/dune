# Diodo: as extensões do ngspice-47 e o `LEVEL=3`

O diodo do Dune é o do 3f5 (`diode.rs`, com as correções de ruptura da
D-0040). Este documento cobre o que o ngspice-47 acrescenta
(`src/spicelib/devices/dio/`, Modified BSD: "Copyright 1990 Regents of
the University of California. Author: 1985 Thomas L. Quarles. Modified:
2000 AlansFixes. Modified by Paolo Nenzi 2003, Dietmar Warning 2012 and
Arpad Buermen 2025"), portado em `crates/dune-core/src/dioext.rs`
(2026-11-14, Fase 8; lacuna G11 do `docs/pdk-parser.md`).

## O que depende de `LEVEL` e o que vale para todo diodo

Lido no C (`diosetup.c`, `diotemp.c`, `dioload.c`): quase nada depende de
`LEVEL`. O ngspice-47 aplica a **todo** diodo, nível 1 inclusive:

- corrente de parede `JSW` com `PJ` (emissão própria `NS` só quando
  dada; sem `NS` a parede segue a característica do fundo, inclusive a
  ruptura);
- recombinação `ISR`/`NR` com o fator de geração
  `((1−vd/PB)²+0,005)^(M/2)` (só quando `ISR` é dado);
- tunelamento `JTUN`/`JTUNSW`/`NTUN`/`XTITUN`/`KEG` (só quando dados);
- joelhos `IKF`/`IK`, `IKR`, `IKP` (só quando dados e ≥ `CKTepsmin`;
  abaixo disso o C desliga o efeito com aviso — o `ikr = 0` dos cartões
  do sky130 cai aqui);
- `NBV`/`NZ` (default `N`) na ruptura;
- capacitância de parede `CJSW`/`CJP`, `PHP`/`VJSW`, `MJSW` (0,33),
  `FCS` (0,5);
- temperatura: `TLEV` 0/1/2 (`EG` default 1,16 com `TLEV=2`, banda com
  `GAP1`/`GAP2`), `TLEVC` 0/1 (`CTA`/`CTC`, `CTP`, `TPB`/`TVJ`,
  `TPHP`), `TCV`/`TBV1`, `TRS`/`TRS1`, `TRS2`, `TTT1`, `TTT2`, `TM1`,
  `TM2`; `TREF` como sinônimo de `TNOM`;
- `AREA` e `PJ` de modelo como defaults da instância; piso
  `IS ≥ CKTepsmin`.

Só o `LEVEL=3` acrescenta:

- `W`/`L` de instância (os dois dados): `AREA = (W+XW)·(L+XW)·M·scale²`,
  `PJ = (2(W+XW)+2(L+XW))·M·scale`, com o `.option scale`;
- as capacitâncias de sobreposição de metal e poli,
  `εSiO2/XOM·M·(WM·scale+XM)·(LM·scale+XM)` e o análogo com
  `XOI`/`XP`/`WP`/`LP` (`LM LP WM WP` de instância ou modelo), somadas à
  carga como `(cmetal+cpoly)·vd`;
- a corrente de ruptura por área: `cbv = IBV·AREA·M` (no nível 1,
  `M·IBV`).

Sem nenhum parâmetro do ngspice (e com no máximo `LEVEL=1`) o diodo segue
o caminho do 3f5 de antes, **bit a bit** — os decks antigos não mudam. A
presença de qualquer parâmetro estendido no `.model` (ou de `PJ`, `W`,
`L`, `LM`, `LP`, `WM`, `WP` na instância) leva a instância ao caminho do
ngspice-47, que com tudo nos defaults dá os mesmos números do 3f5 (teste
`dioext::defaults_reduce_to_the_3f5_path`, 1e-12 relativo) salvo o
`vcrit` da limitação, que o 3f5 calcula sem a área e o ngspice com
`IS·AREA·M` (só muda o caminho do Newton).

## Mapa C → Rust

| ngspice-47 | Dune |
|---|---|
| `dio.c` `DIOmPTable`/`DIOpTable` (nomes e sinônimos) | `elaborate.rs::diode_model` e o ramo `ir::Device::Diode` |
| `diosetup.c` (defaults, pisos, `AREA`/`PJ`/nível 3) | `dioext::DioModelExt::default`, `dioext::setup_geometry`, início de `dioext::temp_update` |
| `diotemp.c` `DIOtempUpdate` | `dioext::temp_update` (sem as derivadas `_dT` do self-heating) |
| `dioload.c` correntes | `dioext::DioExtT::current` |
| `dioload.c` cargas | `dioext::DioExtT::depletion` (+ a difusão `TT·cd` em `diode.rs`) |
| `dioacld.c`, `diopzld.c`, `dionoise.c` | sem mudança: leem `gd`, `capd` e `cd` do estado, que já incluem as extensões |
| `diodset.c` (`.disto`) | recusado para diodo com extensões |

Os parâmetros estendidos não são campos do `DiodeModel` público: o
elaborador grava, por instância, um `DioExtInst` (modelo estendido,
geometria, `scale`) em `Circuit::dio_ext`, um canal crate-privado como o
`Circuit::mult`. **Nenhuma API pública muda.**

## Parâmetros

Todos os do `DIOmPTable`/`DIOpTable` do ngspice-47 estão implementados,
inertes como no C, ou recusados pelo nome:

- **Implementados** (modelo): `level` (1, 3), `is js jsw isw tnom tref rs
  trs trs1 trs2 n ns tt ttt1 ttt2 cjo cj0 cj vj pb m mj tm1 tm2 cjp cjsw
  php vjsw mjsw ikf ik ikr ikp nbv nz area pj tlev tlevc eg gap1 gap2 xti
  cta ctc ctp tpb tvj tphp jtun jtunsw ntun xtitun keg kf af fc fcs bv vb
  vrb var ibv ib tcv tbv1 isr nr lm lp wm wp xom xoi xm xp xw`, a flag
  `d`. (Instância): `area pj perim w l m lm lp wm wp off ic temp dtemp`.
- **Inertes como no C**: os limites SOA `fv_max bv_max id_max te_max
  pd_max` (só avisam com `.option warn`); `rth0 cth0` (só agem com
  self-heating); `sens_area` (flag da sensibilidade antiga); `vp` com
  `TT = 0` (o C só cria o nó `qp` com `VP ≠ 0` e `TT ≠ 0`).
- **Recusados com erro** (no elaborador, só quando uma instância usa o
  modelo — as bibliotecas dos PDKs trazem muitos): `level` ≠ 1, 3 (o 2 o
  próprio ngspice recusa); `rsw` (diodo de parede separado, nó
  `internal_sw`); `vp ≠ 0` com `TT ≠ 0` (recuperação reversa suave, nó
  `qp`); `tlevc` ≠ 0, 1 (o C deixa potencial e capacitância sem valor);
  `tlev` > 2; `cond` (só de saída); a flag `thermal` da instância e um
  diodo de três nós (self-heating — nó térmico, derivadas `_dT`).
- `scalm` (cartões do sky130) não é parâmetro do diodo no ngspice
  ("unrecognized parameter - ignored"), nem no Dune (nota).

As travas do 3f5 (`M ≤ 0,9`, `EG ≥ 0,1`, `FC ≤ 0,95`, com nota) valem
também no caminho estendido; o ngspice-47 não as tem (contrato do 3f5,
D-0002; nenhum cartão dos PDKs chega perto).

## Fidelidade: o que o C faz e o Dune repete

Portado literalmente, mesmo parecendo engano:

1. **`M` duas vezes no nível 3 com `W`/`L`**: o `diosetup.c` já põe o
   `M` na área e no perímetro, e todas as correntes, capacitâncias e
   joelhos multiplicam de novo por `M` (`IS·AREA·M`…): `m=2` equivale a
   4 dispositivos. Medido (`dio3.cases l3_wl_m`, `regress/dio3_op` d12).
2. **Derivada da recombinação**: `gdb_rec = gdb_rec·gen_fac +
   cdb_rec·gen_fac_vd` usa o `cdb_rec` **já** multiplicado por
   `gen_fac` (o certo seria o de antes). Só muda o Jacobiano, não a
   corrente: o ponto final é o mesmo, a convergência não.
3. **Parede na ruptura sem `NS`**: com `JSW` sem `NS` e sem `RSW`, o ramo
   de ruptura avalia a parede em `vdsw`, que fora do caminho com `RSW`
   nunca recebe a tensão do diodo e vale 0: a parede contribui
   `−csatsw·exp(−BV/(NBV·vt))` ≈ 0 em vez da sua ruptura (o `cbv` do
   casamento usa a soma das duas correntes de saturação mesmo assim).
4. **Carga de parede em polarização direta** (`vd ≥ FCS·PHP`): usa o
   `tF1` da junção de **fundo** (não existe um `tF1SW`). Com `PHP`/`MJSW`
   diferentes dos do fundo a carga dá um salto ao cruzar `FCS·PHP`;
   medido: o transiente de `regress/dio3_tran` com `PHP=0,6 MJSW=0,3` e
   `TMAX = 12,5 ps` para com "timestep too small" em t = 11,54 ns no
   ngspice-47 **e** no Dune (mesmo instante, nó 2); o deck usa por isso
   os `PHP`/`MJSW` do fundo.
5. **Unidades das sobreposições**: `XOM`/`XOI` têm default `1e4` com o
   comentário "10k Angstrom", mas a fórmula divide `εSiO2` por eles
   direto, como se fossem metros: `cmetal` sai ~1e10 vezes menor que o
   pretendido. Os cartões dos PDKs têm `LM = WM = LP = WP = 0`.
6. **`TLEVC=1` usa `REFTEMP`**, não `TNOM`: `PB − TPB·(T − 300,15)` e
   `CJO·(1 + CTA·(T − 300,15))`, mesmo com `TREF` dado (sky130 usa
   `tref=30`).
7. **Fator do `TLEV=0/1` para o tunelamento**: `arg1_dT` (só no
   self-heating, fora daqui) esquece o `KEG`; a corrente usa `KEG`
   certo.
8. **Constantes**: o caminho estendido usa as constantes do 3f5 (como o
   resto do Dune); o ngspice-47 usa as do CODATA 2014 — ~3e-5 relativo
   nos resultados, abaixo das tolerâncias.

## Sondas e decks (goldens do ngspice-47)

Sondas em `.claude/scratch/dev-probes/s2/dio3.cases` (29 casos de `.op`,
todos dentro de 1e-3 relativo do ngspice-47). Decks:

| deck | cobre | margem |
|---|---|---|
| `regress/dio3_op` | parede com e sem `NS`, `ISR` direta e reversa, `IKF`/`IKP`/`IKR`, tunelamento, `NBV`, ruptura com parede e `m`, `TRS`/`TREF`/`DTEMP`, nível 3 com `W`/`L`/`XW`/`scale`/`m` e com `AREA`/`PJ`, ruptura de nível 3 | 0,014 |
| `regress/dio3_dc` | varredura de −10 a 1,2 V por ruptura, reversa e direta (parede própria, `ISR`, joelhos, tunelamento, nível 3 com `TLEV=1`/`TCV`) | 0,270 |
| `regress/dio3_temp` | `.dc temp` de −40 a 125 °C: `TLEV` 0/1/2, `TLEVC` 0/1, `TCV`, `TRS`, `TM1`, nível 3 estilo sky130 | 0,101 |
| `regress/dio3_ac` | capacitância de parede (reversa e além de `FCS·PHP`), `TLEVC=1` a 60 °C, `TT`/`TTT1`, sobreposições de nível 3 | 0,037 |
| `regress/dio3_tran` | chaveamento com `TT`, parede, `ISR`, `IKF`, a 50 °C (`compat=spice3`) | 0,001 |
| `regress/dio3_noise` | shot da corrente total, flicker com `AF ≠ 1` e `m`, térmico de `RS` com `TRS` | 0,474 |

## PDK

Diodos do sky130 lidos direto da biblioteca (`sky130.lib.spice tt`;
bancada em `.claude/scratch/dev-probes/s2/sky130_dio_dc.cir`, `.dc` de
−13 a 0,9 V por `pw2nd_05v5` e `pd2nw_05v5`, nível 3 com `tlevc=1`,
`ik`, `ikr=0`, `vb`): contra o ngspice-47, `i(v1)` 1,3e-5 e `i(v2)`
3,9e-4 relativos ao máximo. Antes desta porta o Dune recusava o `pj=` da
instância ("unknown parameter (pj)"); sem ele (`sky130_dio_d1.cir`, só o
`pw2nd_05v5` com a área do modelo) simulava nível 1 sem parede, sem
`TLEVC`, com a área 1 em vez da `area = 1e12` do modelo e o `IBV`
absoluto: erro de 100 % do máximo, contra 1,3e-5 agora.

## Pendências

- `.disto` com diodo estendido: recusado (o `diodset.c` do ngspice-47 tem
  outra forma que a do 3f5 portada no Dune — limiar `−3·vte`, ramo
  reverso com `g2 = −4·gd/vd` — e a parede entra com o `tDepCap` do
  fundo).
- `RSW`, `VP` (com `TT ≠ 0`) e self-heating: recusados; só o IHP usa
  `rsw`, e o IHP não roda no Dune (OSDI).
- `.sens`: os parâmetros estendidos não entram na varredura (como antes,
  `KNOWN_DIFFS.md`); uma perturbação de `IS`, `N`… num diodo estendido
  refaz a temperatura pelo caminho do ngspice.
