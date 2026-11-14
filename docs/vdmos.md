# VDMOS — o MOSFET de potência do ngspice-47

Porte do modelo `vdmos` do ngspice-47 (`src/spicelib/devices/vdmos/`,
licença Modified BSD: © 1990 Regents of the University of California;
Thomas L. Quarles 1985; AlansFixes 2000; VDMOS: Holger Vogt 2018,
Dietmar Warning 2020; setup de distorção: Jaijeet S. Roychowdhury 1988).
O SPICE3f5 não tem VDMOS: aqui o contrato **e** o oráculo são o
ngspice-47 (mesma situação do BSIM3).

Código: `crates/dune-core/src/vdmos/` (`model.rs`, `mod.rs`, `load.rs`,
`tests.rs`); sintaxe em `crates/dune-netlist/src/{parse,elem}.rs`;
elaboração em `crates/dune-core/src/elaborate.rs`
(`vdmos_model`/`vdmos_instance`) e `circuit.rs` (`Spec::Vdmos`).

## Mapa C → Rust

| C (ngspice-47) | Rust | Observação |
|---|---|---|
| `vdmos.c` (tabelas `IFparm`) | `VdmosModel::set`, `elaborate::vdmos_instance` | nomes e aliases (`vth0`, `vtotc`, `bex`) do C |
| `vdmosmpar.c` | `VdmosModel::set` | flags de tipo `vdmosn`/`vdmosp`/`vdmos` |
| `vdmospar.c` | `elaborate::vdmos_instance` | `m`, `off`, `icvds`, `icvgs`, `ic`, `temp`, `dtemp`, `thermal` |
| `vdmosset.c` (defaults, nós internos, TSTALLOC) | `VdmosModel::new`/`prepare`, `circuit.rs` (`vdmos_internal`, passo 2), `VdmosI::new`, `Device::alloc` | ordem dos nós internos e das posições igual à do C |
| `vdmostemp.c` (`VDMOStempUpdate`) | `model::VdmosTemp::new` | função pura da temperatura; o self-heating a chama a cada load |
| `vdmosload.c` | `load.rs` (`VdmosI::do_load`) | transcrição comando a comando |
| `vdmosacld.c` | `Device::load_ac` | |
| `vdmospzld.c` | `Device::load_pz` | |
| `vdmosnoi.c` | `Device::noise_dens` | sufixos `_rd _rs _id _1overf` (os do MOS) |
| `vdmosdset.c`, `vdmosdist.c` | `Device::disto_setup`/`disto_load` | carga com `disto.rs` (x = vgs, y = vds, z = 0) |
| `vdmosconv.c` | `Device::conv_test` | |
| `vdmostrun.c` | `Device::trunc` | |
| `vdmosic.c` | `Device::set_ic` | |
| `devsup.c`: `DevCapVDMOS`, `DEVlimitlog` | `model::dev_cap`, `model::limitlog` | `DEVfetlim`/`DEVlimvds`/`DEVpnjlim` reaproveitados (`mos1.rs`, `diode.rs`) |
| `inp2m.c`, `inpcom.c::inp_vdmos_model` | `dune-netlist`: `elem::mosfet`, `parse.rs` | 3 a 5 nós; `pchan` |
| `vdmosask.c`, `vdmosmask.c` | — | o Dune não tem consultas `@dev[param]` |
| `vdmossoachk.c` | — | **fora de escopo** (parâmetros SOA aceitos e inertes) |
| `vdmosbindCSC.c` | — | ligação KLU, sem equivalente |

## Sintaxe

```
.model NOME VDMOS (pchan|nchan  KP=… VTO=… …)
MXXX ND NG NS <TJ <TC>> NOME <M=> <OFF> <IC=VDS,VGS> <ICVDS=> <ICVGS=>
+    <TEMP=> <DTEMP=> <THERMAL>
```

- Tipo: `vdmos` (também `vdmosn`/`vdmosp`, as formas que o front end do
  ngspice escreve). `pchan` em qualquer lugar do cartão torna-o canal P;
  `nchan`/`pchan` e as anotações de datasheet `ron=`, `vds=`, `qg=`,
  `mfg=` são descartadas, como no `inp_vdmos_model`.
- O `M` do **modelo** é o coeficiente de gradação do diodo de corpo; o
  `M` da **instância** é o multiplicador paralelo.
- Nós: o `INP2M` termina a lista no primeiro nome de modelo; para VDMOS
  aceita 3 a 5 nós (dreno, porta, fonte, temperatura de junção, de
  encapsulamento). O parser do Dune só lê um 4º/5º nó como tal quando o
  modelo VDMOS vem logo depois, de modo que um cartão MOS de 4 nós lê
  como antes qualquer que seja a tabela de modelos.
- `thermal` exige os 5 nós (erro com span; ver KNOWN_DIFFS).
- `TEMP` e `DTEMP` juntos: o `VDMOSsetup` avisa e fica com `DTEMP`
  (inverso dos outros dispositivos); temperatura = `CKTtemp + DTEMP`.

## Parâmetros do modelo (nomes e defaults do ngspice-47)

| Parâmetro | Default | Efeito |
|---|---|---|
| `VTO`/`VTH0` | `3·tipo` | limiar |
| `KP` | `15 + 5·tipo` (20 n, 10 p) | transcondutância |
| `PHI` | 0,6 | só na distorção (`tPhi`); ≤ 0 é fatal |
| `LAMBDA`, `THETA` | 0, 0 | modulação de canal, degradação de mobilidade |
| `MTRIODE` | 1 | multiplicador de vds no triodo |
| `SUBSHIFT`, `KSUBTHRES` | 0, 0,1 | inversão fraca |
| `RD`, `RS`, `RG` | 0 | resistências (nó interno quando > 0) |
| `RQ`, `VQ` | — | quase-saturação (só com os dois) |
| `RDS` | — | shunt D-S (`M/RDS`; sem ele, 1e-15 S fixos) |
| `CGDMIN`, `CGDMAX`, `A`, `CGS` | 2e-11, 2e-9, 1, 1,4e-9 | capacitâncias de porta |
| `IS`, `N`, `RB` | 1e-14, 1, 0 | diodo de corpo |
| `VJ`, `CJO`, `M`, `FC` | 0,8, 5e-10, 0,5, 0,5 | depleção (`FC` ≤ 0,95) |
| `TT`, `VP` | 0, 0 | difusão; recuperação suave com `VP ≠ 0` e `TT ≠ 0` |
| `BV`, `IBV`, `NBV` | —, 1e-10, 1 | ruptura |
| `EG`, `XTI` | 1,11 (≥ 0,1), 3 | lei de temperatura de `IS` |
| `TNOM` | `.options tnom` | |
| `TCVTH`/`VTOTC`, `MU`/`BEX` | 0, −1,5 | `VTO + TCVTH·ΔT`, `KP·(T/TNOM)^MU` |
| `TEXP0`, `TEXP1` | (1,5), 0,3 | `RD·(T/TNOM)^TEXP0` se dado; `RQ·(T/TNOM)^TEXP1` |
| `TRD1/2`, `TRG1/2`, `TRS1/2`, `TRB1/2` | 0 | polinômios de temperatura das resistências |
| `TKSUBTHRES1/2` | 0 | idem de `KSUBTHRES` |
| `RTHJC`, `RTHCA`, `CTHJ` | 1 (precisa ser dado), 23, 10e-6 | self-heating |
| `KF`, `AF` | 0, 1 | flicker |
| `VGS_MAX` … `DERATING` | 1e99…, `ID_MAX` 0 | SOA, inertes |

## Equações

Canal (`vdmosload.c`), com `vgs`/`vds` normalizados pelo tipo e o modo
(`vds ≥ 0` normal):

- `von = tVth·tipo`, `vgst = vgs(ou vgd) − von`, `vdsat = max(vgst, 0)`;
- `t0 = 1 + LAMBDA·vds` (vds **com sinal**), `t1 = 1 + THETA·vdsat`,
  `β' = β·t0/t1`;
- inversão fraca: `vgst ← KS·ln(1 + exp((vgst − SUBSHIFT)/KS))`;
- `vdss = vds·MTRIODE·modo`; saturação (`vgst ≤ vdss`):
  `Id = β'·vgst²/2`; triodo: `Id = β'·vdss·(vgst − vdss/2)`.

Quase-saturação (d'Alessandro, simplificado): no modo normal,
`rd = RD/M·f(T) + RQ/M·g(T)·vdsn/(vdsn + |VQ|)` com `vdsn` a tensão
dreno-fonte **externa** do iterado anterior, e `gdrain = 1/rd + gmin`;
não há estampa da derivada (ponto fixo, como no C).

Capacitâncias (`DevCapVDMOS`): `s = (CGDMAX − CGDMIN)/(1 + π/2)`,
`Cgd = s·tanh(A·vgd) + CGDMAX − s` para `vgd > 0`,
`s·atan(A·vgd) + CGDMAX − s` abaixo; `Cgs = CGS`; integradas à Meyer
(média de dois pontos), como o MOS1.

Diodo de corpo (anodo no lado da fonte, catodo no dreno externo):
`IS·(exp(vd/(N·vt)) − 1)` acima de `−3·N·vt`, a cauda
`−IS·(1 + (3·N·vt/(e·vd))³)` abaixo, e a ruptura
`−IS·exp(−(BV' + vd)/(NBV·vt))` com o joelho `BV'` casado em `IBV` na
temperatura (laço de 25 iterações do `vdmostemp.c`). Carga de depleção
com `VJ`/`CJO`/`M`/`FC` e de difusão `TT·id`; com recuperação suave a
difusão sai e entra o nó `qp`: `qp/VP + TT/VP·dqp/dt = TT/VP·id` e o
diodo ganha `(1 − VP)/TT·dqp/dt`.

Self-heating (`thermal` + `RTHJC` dado): o nó `tj` carrega a temperatura
de junção em °C; a potência do canal, de `RD`, `RS`, do diodo e de `RB`
entra nele linearizada; `CTHJ` liga `tj` à terra (carga Meyer),
`RTHJC` liga `tj` a `tc`, `RTHCA` liga `tc` a um nó interno mantido à
temperatura ambiente (o `TEMP=` da instância, ou `CKTtemp`; escalada por
`srcFact` só no TRANOP) por um ramo de fonte. Em cada iteração o modelo
inteiro é reavaliado na temperatura `tj + 273,15` (`VDMOStempUpdate`).

Ruído: térmico de `RD`, `RS` e do canal `(2/3)|gm|` a **`CKTtemp`**
(o `NevalSrc` comum, não o de temperatura de instância do MOS1), vezes
`Tj/CKTtemp` sob self-heating; flicker `KF·|Id|^AF/(f·Cox²)` com o óxido
de 100 nm do setup. `RG`, `RB` e o diodo são sem ruído no C.

## Fidelidade: o que o C faz e o Dune repete

Portado literalmente, mesmo parecendo engano (o oráculo é o ngspice):

1. `vdmosconv.c` subtrai `gm·Δvgs` (o load soma) no `cdhat` do modo
   normal; o teste do diodo lê `rhsOld`, então só rejeita depois de
   limitação.
2. `t0 = 1 + LAMBDA·vds` usa o vds com sinal: no modo inverso o fator
   **reduz** a corrente.
3. Self-heating: `dvgst/dT = +tipo·TCVTH` (o limiar sobe com `TCVTH`, o
   sinal correto seria o oposto); `dvdsat/dT` é testado com o `vgst` já
   suavizado (sempre > 0).
4. AC com self-heating (`vdmosacld.c`): sinais de `gTtt`/`gTtg`/`gTtdp`
   opostos aos do load, `cgT`/`cdT` nunca escritos (zero), capacitância
   térmica negativa no modo inverso, e nenhuma estampa térmica de
   `RD`/`RS`/diodo.
5. Distorção: o setup é o do MOS1 (derivadas sem inversão fraca nem
   `THETA`, `KSUBTHRES` sem temperatura, Meyer com `Cox` do óxido de
   100 nm × `M`), `vgs` lido na porta **externa**, e o diodo de corpo
   não entra; o `D_2F1MF2` lê os núcleos de primeira ordem como os
   arquivos do MOS.
6. `VDMOSsetup` grava o default 1e99 de `ID_MAX` em `PD_MAX` (SOA,
   inerte aqui).
7. `TEMP` + `DTEMP`: fica `DTEMP`.

Reescrito, com motivo (linhas em `corpus/KNOWN_DIFFS.md`):

- `thermal` com menos de 5 nós: erro em toda instância (o C só confere
  a primeira linha do deck).
- Ramo `VdevTemp` tipado como nó no teste de convergência (o C usa
  corrente); mesmo nome de vetor.
- Nós `tj`/`tc` sem self-heating: diagonal unitária (0 V), o que o
  Sparse do ngspice entrega por nunca vê-los.
- `DEVlimitlog` com NaN zera sempre (o C, uma vez por processo).
- `.pz` com self-heating ou recuperação suave: recusa (o C deixa linhas
  vazias).
- Bypass (`CKTbypass`): não portado, como em todo o Dune.
- Constantes físicas: as do 3f5 (KNOWN_DIFFS, D-0002); o ngspice usa
  CODATA. A diferença de `k/q` (~3e-5) aparece em ~5e-4 relativo nas
  correntes do diodo em polarização direta forte — dentro das réguas.

## Sondas (`corpus/regress/vdmos_*.cir`, goldens do ngspice-47)

| Deck | Análise | Cobre |
|---|---|---|
| `vdmos_dc` | `.dc` | famílias n/p, RD/RS/RG, LAMBDA, THETA, MTRIODE, inversão fraca, quase-saturação, RDS, M=2, diodo direto e ruptura |
| `vdmos_temp` | `.dc` | temp=85 vs TNOM=25, todos os coeficientes de temperatura, TEMP/DTEMP, TEXP0, nós de temperatura sem `thermal` |
| `vdmos_tran` | `.tran` | chaveamento com Cgd(vgd)/Cgs, canais N e P |
| `vdmos_rr` | `.tran` | recuperação suave (VP, nó `qp`) contra a carga TT simples |
| `vdmos_ac` | `.ac` | capacitâncias no ponto de operação, quase-saturação, canal P |
| `vdmos_noise` | `.noise` | térmico de RD/RS/canal, flicker |
| `vdmos_pz` | `.pz` | Cgd/Cgs e diodo no `s` complexo |
| `vdmos_disto` | `.disto` | intermodulação n e p |
| `vdmos_thermal_dc` | `.dc` | rede térmica estática (cartão IRFP240 do exemplo do ngspice) |
| `vdmos_thermal_tran` | `.tran` | `CTHJ` integrado, atualização de temperatura por iteração |
| `vdmos_thermal_ac` | `.ac` | estampas térmicas do AC, `TEMP=` como ambiente da rede |

Transientes no perfil `spice3` (`.options compat=spice3`), como todo
transiente comparado com o ngspice. Os decks de self-heating amarram o
encapsulamento a um dissipador de 27 °C: com `tc` só ligado por
`RTHCA` o próprio ngspice-47 não fecha o ponto de operação (sonda fora
do corpus).

## Pendências

- `.sens`: o VDMOS não tem porta de sensibilidade (como os demais MOS do
  Dune); a análise recusa alto.
- Consultas de operação (`@m1[id]`, `vdmosask.c`) e checagem SOA.
