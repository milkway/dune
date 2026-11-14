# VBIC — o bipolar de quatro terminais do ngspice-47

Porte do modelo `vbic` do ngspice-47 (`src/spicelib/devices/vbic/`,
licença Modified BSD: © 1990 Regents of the University of California;
Thomas L. Quarles 1985; autor do modelo: Colin McAndrew, Motorola, 1995;
implementação no Spice3: Dietmar Warning, DAnalyse GmbH, 2003). É o VBIC
1.2 eletrotérmico de quatro terminais (`vbic_4T_et_cf`). O SPICE3f5 não
tem VBIC: o contrato **e** o oráculo são o ngspice-47 (como no BSIM3 e no
VDMOS). Não fica atrás da feature `bsim` (licença BSD pura).

Código: `crates/dune-core/src/vbic/` (`model.rs`, `fj.rs`, `load.rs`,
`mod.rs`, `tests.rs`); sintaxe em `crates/dune-netlist/src/elem.rs`
(`bjt`: até 5 nós); elaboração em `crates/dune-core/src/elaborate.rs`
(`bjt_level`, `vbic_model`, `vbic_instance`) e `circuit.rs`
(`Spec::Vbic`, `vbic_internal`).

## Mapa C → Rust

| C (ngspice-47) | Rust | Observação |
|---|---|---|
| `vbic.c` (tabelas `IFparm`) | `model::KEYWORDS`/`SOA_KEYWORDS`, `elaborate::vbic_instance` | nomes e aliases do C (`tref`, `v0`, `cbe0`, `cbc0`, `qc0`, `xv0`, `xrc`, `xrb`, `bvbe`…) |
| `vbicmpar.c` | `VbicModel::set` | `R_MIN = 0,01 Ω` nas sete resistências dadas; `npn`/`pnp` como flags; `selft` inteiro |
| `vbicparam.c` | `elaborate::vbic_instance` | `area`, `off`, `ic` (1 ou 2 valores), `icvbe`, `icvce`, `temp`, `dtemp`, `m` |
| `vbicsetup.c` (defaults, nós, TSTALLOC) | `model::DEFAULTS`, `VbicModel::{selfheat, excess_phase, block}`, `circuit.rs` (`vbic_internal`, passo 2), `Device::alloc` | ordem dos nós internos e das posições igual à do C |
| `vbictemp.c` (`VBICtemp`, `vbic_4T_et_cf_t`) | `model::temp_map`, `model::load_block`, `VbicI::new` (`tVcrit`) | |
| `vbicload.c`: `VBICload` | `load.rs` (`VbicI::do_load`) | transcrição comando a comando |
| `vbicload.c`: `vbic_4T_et_cf_fj` | `fj.rs` | tradução **mecânica** (ver abaixo) |
| `vbicacld.c` | `Device::load_ac` → `VbicI::small_signal` | |
| `vbicpzld.c` | `Device::load_pz` → `VbicI::small_signal` | as mesmas estampas com `s` no lugar de `jω` |
| `vbicnoise.c` | `Device::noise_dens` | sufixos `_rc _rci _rb _rbi _re _rbp _rs _ic _ib _ibep _iccp _1overfbe _1overfbep` |
| `vbicconv.c` | `Device::conv_test` | |
| `vbictrunc.c` | `Device::trunc` | |
| `vbicgetic.c` | `Device::set_ic` | |
| `devsup.c`: `DEVpnjlim`, `DEVlimitlog` | `diode::pnjlim`, `vdmos::limitlog` | reaproveitados |
| `inp2q.c`, `inpdomod.c` | `dune-netlist::elem::bjt`, `elaborate::bjt_level` | 3 a 5 nós; `LEVEL` 4 ou 9 |
| `vbicask.c`, `vbicmask.c` | — | o Dune não tem consultas `@q[param]` |
| `vbicsoachk.c` | — | **fora de escopo** (parâmetros SOA aceitos e inertes) |
| `vbicbindCSC.c` | — | ligação KLU, sem equivalente |

O C não tem `DEVdisto` nem `DEVsen*`: `.disto` e `.sens` com VBIC recusam
alto (o ngspice trataria o VBIC como linear no `.disto`).

### `fj.rs`: tradução mecânica

`vbic_4T_et_cf_fj` (2,6 mil linhas, ~700 variáveis) é código gerado no
próprio C (equações e derivadas parciais escritas por ferramenta). Foi
traduzido por um script, comando por comando, com só três reescritas
sintáticas: `(*X)`/`*X=` (parâmetros por ponteiro) viram variáveis
locais, `if(c){` vira `if c {`, literais inteiros viram `f64`
(`2` → `2.0`, fora dos índices `p[k]`). `pow`/`exp`/`log`/`sqrt` são
funções locais que chamam `powf`/`exp`/`ln`/`sqrt` (a mesma libm). As
entradas são `FjBias` (as 18 tensões de ramo), as 101 saídas `FjOut`, com
os nomes do C. Todas as variáveis são iniciadas com 0; uma compilação de
verificação com declarações sem valor inicial confirmou que nenhuma é
lida antes de atribuída em qualquer caminho (o zero nunca é observável).

## Sintaxe

```
.model NOME NPN|PNP (LEVEL=4 …)      (ou LEVEL=9)
QXXX NC NB NE <NS <NT>> NOME <AREA> <OFF> <IC=VBE,VCE> <ICVBE=> <ICVCE=>
+    <AREA=> <M=> <TEMP=> <DTEMP=>
```

- `LEVEL` (arredondado como no `INPfindLev`): 0–2 Gummel-Poon, 4 e 9
  VBIC, 8 HICUM/L2 (não implementado: erro só quando uma instância o
  usa), outros erro. `LEVEL` não é parâmetro do modelo (o `INPgetMod` o
  descarta; o cartão Gummel-Poon também deixou de anotá-lo).
- Nós: o `INP2Q` termina a lista no primeiro nome de modelo; o VBIC
  aceita até 5 (coletor, base, emissor, substrato, nó térmico), os que
  faltam vão à terra. O parser do Dune só lê um 5º nó como tal quando o
  modelo vem logo depois; o elaborador recusa 5 nós num modelo
  Gummel-Poon ("too many nodes", como o `INP2Q`).
- Área: o valor posicional é aplicado **depois** das palavras-chave no
  `INP2Q`, então vence `AREA=`.
- `TEMP` e `DTEMP` juntos se **somam** (`VBICtemp`); `TEMP` em °C.
- Parâmetros de instância num cartão `.model` (defaults de instância do
  ngspice, `IF_SET`): recusados com mensagem, em vez de ignorados.

## Parâmetros do modelo (nomes e defaults do ngspice-47)

Índice no bloco `p[108]` do C entre parênteses (é como o load os lê).

| Parâmetro | Default | Efeito |
|---|---|---|
| `TNOM`/`TREF` (0) | 27 (°C; **não** segue `.options tnom`) | temperatura de extração |
| `RCX` (1), `RCI` (2) | 0, 0,1 | coletor extrínseco; epi (Kull) |
| `VO`/`V0` (3), `GAMM` (4), `HRCF` (5) | 0, 0, 1 | quase-saturação de Kull |
| `RBX` (6), `RBI` (7) | 0, 0,1 | base extrínseca; intrínseca (÷ `qb`) |
| `RE` (8), `RS` (9), `RBP` (10) | 0, 0, 0,1 | emissor, substrato, base parasita (÷ `qbp`) |
| `IS` (11), `NF` (12), `NR` (13) | 1e-16, 1, 1 | transporte |
| `FC` (14) | 0,9 | limite de polarização direta das depleções |
| `CBEO`/`CBE0` (15), `CBCO`/`CBC0` (20) | 0, 0 | capacitâncias de sobreposição (nós externos) |
| `CJE` (16), `PE` (17), `ME` (18), `AJE` (19) | 0, 0,75, 0,33, −0,5 | depleção B-E (`AJE ≤ 0`: forma SPICE; `> 0`: suavizada) |
| `CJC` (21), `QCO`/`QC0` (22) | 0, 0 | depleção B-C; carga do epi (`QCO·Kbci`, `QCO·Kbcx`) |
| `CJEP` (23), `PC` (24), `MC` (25), `AJC` (26) | 0, 0,75, 0,33, −0,5 | depleção B-C extrínseca (parasita B-E) |
| `CJCP` (27), `PS` (28), `MS` (29), `AJS` (30) | 0, 0,75, 0,33, −0,5 | depleção S-C |
| `IBEI` (31), `WBE` (32), `NEI` (33) | 1e-18, 1, 1 | B-E ideal; `WBE` reparte entre `Vbei` e `Vbex` |
| `IBEN` (34), `NEN` (35) | 0, 2 | B-E não ideal |
| `IBCI` (36), `NCI` (37), `IBCN` (38), `NCN` (39) | 1e-16, 1, 0, 2 | B-C |
| `AVC1` (40), `AVC2` (41) | 0, 0 | avalanche fraca |
| `ISP` (42), `WSP` (43), `NFP` (44) | 0, 1, 1 | transistor parasita (0: desligado) |
| `IBEIP` (45), `IBENP` (46) | 0, 0 | B-E parasita |
| `IBCIP` (47), `NCIP` (48), `IBCNP` (49), `NCNP` (50) | 0, 1, 0, 2 | B-C parasita (substrato) |
| `VEF` (51), `VER` (52) | 0, 0 (infinito) | Early |
| `IKF` (53), `IKR` (54), `IKP` (55) | 0, 0, 0 (infinito) | joelhos |
| `TF` (56), `QTF` (57), `XTF` (58), `VTF` (59), `ITF` (60) | 0 | tempo de trânsito direto |
| `TR` (61) | 0 | tempo de trânsito reverso (e da parasita) |
| `TD` (62) | 0 | excesso de fase (`> 0` cria `xf1`/`xf2`) |
| `KFN` (63), `AFN` (64), `BFN` (65) | 0, 1, 1 | flicker |
| `XRE`, `XRBI`, `XRCI`, `XRS`, `XVO`/`XV0` (66–70) | 0 | expoentes de temperatura |
| `EA`, `EAIE`, `EAIC`, `EAIS`, `EANE`, `EANC`, `EANS` (71–77) | 1,12 | energias de ativação |
| `XIS`, `XII`, `XIN` (78–80) | 3 | expoentes de temperatura das correntes |
| `TNF` (81), `TAVC` (82) | 0 | `NF`/`NR`·(1+ΔT·TNF), `AVC2`·(1+ΔT·TAVC) |
| `RTH` (83), `CTH` (84) | 0, 0 | self-heating (`RTH > 0` dado); `CTH` sobe a 1e-12 com `RTH` dado |
| `VRT` (85), `ART` (86) | 0, 0,1 | reach-through da depleção B-C |
| `CCSO` (87) | 0 | capacitância C-S fixa |
| `QBM` (88) | 0 | `< 0,5`: `qb` do VBIC; `≥ 0,5`: o do SGP |
| `NKF` (89) | 0,5 | expoente do joelho |
| `XIKF`, `XRCX`/`XRC`, `XRBX`/`XRB`, `XRBP` (90–93) | 0 | expoentes de temperatura |
| `ISRR` (94), `XISR` (95), `DEAR` (96) | 1, 0, 0 | `IS` reverso separado |
| `EAP` (97) | 1,12 | ativação de `ISP` |
| `VBBE` (98), `NBBE` (99), `IBBE` (100) | 0, 1, 1e-6 | ruptura B-E (`VBBE > 0`) |
| `TVBBE1`, `TVBBE2`, `TNBBE` (101–103) | 0 | temperatura da ruptura |
| `EBBE` (104) | 0 | aceito; o load recalcula `exp(−VBBE/(NBBE·Vtv))` e nunca o lê |
| `DTEMP` (105) | 0 | delta de temperatura do **modelo** |
| `VERS` (106), `VREF` (107) | 1,2, 0 | inertes |
| `SELFT` | — | se dado, só `SELFT = 1` (com `RTH > 0`) liga o self-heating |
| `VBE_MAX`/`BVBE` … `VSUBFWD` | 1e99…, 0,2 | SOA, inertes |

## Equações

Tensões de ramo normalizadas pelo tipo (`Vbei = tipo·(v(bi) − v(ei))`
etc.); `Vtv = k·T/q` com as constantes **do próprio C** (1,380662e-23,
1,602189e-19). Correntes (`vbic_4T_et_cf_fj`):

- transporte: `Ifi = IS·(exp(Vbei/(NF·Vtv)) − 1)`,
  `Iri = IS·ISRR·(exp(Vbci/(NR·Vtv)) − 1)`;
- carga de base: `q1z = 1 + qdbe/VER + qdbc/VEF` (cargas de depleção
  **normalizadas**, que dependem de `PE/ME/AJE`, `PC/MC/AJC/VRT/ART` mesmo
  com `CJE = CJC = 0`), `q1` = `q1z` suavizado em 1e-4, `q2 = Ifi/IKF +
  Iri/IKR`; `qb = (q1 + (q1^(1/NKF) + 4·q2)^NKF)/2` (`QBM < 0,5`) ou
  `q1·(1 + (1 + 4·q2)^NKF)/2` (SGP);
- `Itzf = Ifi/qb`, `Itzr = Iri/qb`; com excesso de fase, o `Itzf` passa
  pela rede de Weil-McNamee: `TD·dVxf1/dt + Vxf2 − Itzf = 0`,
  `TD/3·dVxf2/dt + Vxf2 − Vxf1 = 0`, e `Iciei = Vxf2 − Itzr`;
- B-E: `Ibe = WBE·(IBEI·(exp(Vbei/(NEI·Vtv))−1) + IBEN·(…NEN…) −
  IBBE·(exp(−(Vbei+VBBE)/(NBBE·Vtv)) − EBBE))`, `Ibex` com `1 − WBE` em
  `Vbex`;
- B-C com avalanche: `Ibc = Ibcj − Igc`, `Igc = (Itzf − Itzr − Ibcj)·
  AVC1·vl·exp(−AVC2·vl^(MC−1))`, `vl` = `PC − Vbci` suavizado;
- epi (Kull): `Kbci = √(1 + GAMM·exp(Vbci/Vtv))`, `Kbcx` idem em `Vbcx`,
  `Iohm = (Vrci + Vtv·(Kbci − Kbcx − ln((Kbci+1)/(Kbcx+1))))/RCI`,
  `Irci = Iohm/√(1 + derf²)`, `derf = Iohm·RCI/VO/(1 + ½·|Vrci|/(VO·
  HRCF))` (com `√(Vrci²+0,01)`);
- parasita: `Ifp = ISP·(WSP·exp(Vbep/(NFP·Vtv)) + (1−WSP)·exp(Vbci/…)
  − 1)`, `qbp = (1 + √(1 + 4·Ifp/IKP))/2`, `Iccp = (Ifp − Irp)/qbp`;
- resistências: `Irbi = Vrbi·qb/RBI`, `Irbp = Vrbp·qbp/RBP`, as outras
  ôhmicas; cargas: `Qbe = CJE·qdbe·WBE + tff·Ifi/qb` (`tff = TF·(1 +
  QTF·q1)·(1 + XTF·exp(Vbci/(1,44·VTF))·(slTF + mIf²)·sgIf)`),
  `Qbex`, `Qbc = CJC·qdbc + TR·Iri + QCO·Kbci`, `Qbcx = QCO·Kbcx`,
  `Qbep = CJEP·qdbep + TR·Ifp`, `Qbcp = CJCP·qdbcp + CCSO·Vbcp`, e as
  sobreposições `CBEO·Vbe`, `CBCO·Vbc` nos nós externos;
- potência: `Ith = −Σ I·V` de todos os ramos (com `Itzf − Itzr` em
  `Vcei`); `Irth = Vrth/RTH`, `Qcth = CTH·Vrth`.
- Tudo (inclusive `Irth` e `Qcth`) sai multiplicado por `SCALE =
  AREA·M`.

Temperatura (`vbic_4T_et_cf_t`): `rT = Tdev/Tini`, `Tdev = TAMB + DTEMP`
do modelo; resistências `·rT^X…`; correntes de saturação
`·(rT^XI·exp(−EA·(1−rT)/Vtv))^(1/N)`; potenciais de junção pela fórmula
do VBIC (`psiio`/`psiin`); capacitâncias `·(P/P(T))^M`; `NF`/`NR`/`AVC2`/
`VBBE`/`NBBE` lineares (quadrático em `VBBE`). Sob self-heating o load
reaplica o mapeamento (dentro do `fj`) da temperatura do dispositivo
(`p[0]` = temperatura da instância) até `p[0] + Vrth`: identidade em
`Vrth = 0`, de modo que o nó térmico carrega o **aumento** de
temperatura (K).

## Fidelidade: o que o C faz e o Dune repete

Portado literalmente, mesmo parecendo engano (o oráculo é o ngspice):

1. **Excesso de fase num pnp** (`vbicload.c` 1139-1145 e 1283-1298;
   `vbicacld.c`/`vbicpzld.c` idem): as linhas de `xf1`/`xf2` e a coluna
   `Iciei`→`xf2` são estampadas sem o fator `VBICtype`, enquanto o RHS usa
   as tensões normalizadas. Num npn não muda nada; num pnp o ponto fixo do
   Newton deixa de ser a solução das equações: medido, a corrente de
   coletor de um pnp com `TD = 10 ps` sai ~45× a de `TD = 0`, e o ponto
   depende do caminho (o `.dc` do ngspice-47 não termina; o `.op` com e
   sem outros dispositivos dá valores diferentes). Sem deck.
2. **Self-heating num pnp** (`vbicload.c` 1427-1467, a linha `Ith` do nó
   térmico; AC/PZ idem): as derivadas de `Ith` em relação às tensões de
   ramo entram na matriz sem `VBICtype`, e o RHS usa as tensões
   normalizadas: ponto fixo errado. Medido (`regress/vbic_pnp_op`):
   ΔT = 8,54 K onde a potência dissipada (0,64 mW × 300 K/W) dá 0,19 K;
   numa varredura o ngspice-47 para em `Vce = −0,3 V`. O Dune reproduz o
   valor errado (PASS).
3. **Colunas `Vbcp`/`Vcep` de `Ith`** (`vbicload.c` 1448-1451, AC/PZ
   idem): vão ao nó de substrato **externo** (`VBICtempSubsPtr`), não ao
   `subsSI`; com `RS > 0` e corrente de substrato sob self-heating o
   ponto fixo se desloca de `Ith_Vbcp·RS·Irs` (coberto por
   `vbic_thermal_dc`).
4. As colunas `Vrth` das correntes de ramo também não têm `VBICtype`, mas
   o RHS acompanha: só o Jacobiano de um pnp fica errado (convergência).
5. Jacobiano do transiente: `NIintegrate` de `Qbe` e `Qbep` só com a
   derivada própria (`Qbe_Vbei`, `Qbep_Vbep`); `Qbe_Vbci`, `Qbep_Vbci` e
   as derivadas térmicas das cargas ficam de fora (só no AC). As
   correntes de carga são exatas; muda a convergência.
6. `gmin` é somado **três vezes** a `Irci` (em `Vrci`, `Vbci` e `Vbcx`).
7. Ruído (`vbicnoise.c`): a fonte térmica de `RBP` fica entre `emitEI` e
   `emit` (os nós de `RE`; com `RE = 0` ela some), não entre `baseBP` e
   `collCX`; `RS` e `ICCP` ficam **fora** do total da densidade, mas suas
   integrais contam no total integrado; a temperatura das fontes térmicas
   é `CKTtemp + dtemp` da instância (`TEMP − CKTtemp` se `TEMP` dado),
   sem o `DTEMP` do modelo e sem o aquecimento do self-heating; o flicker
   usa `|I/M|^AFN·M`.
8. `VBICtrunc` não inclui `qcth`, `qxf1`, `qxf2` no controle de passo.
9. No `MODEINITPRED` o C prediz `Vxf2` e `Vrth` e logo os sobrescreve com
   `CKTrhsOld` (a predição desses dois é morta); as cópias state1→state0
   do preditor são portadas.
10. `VBICtVcrit` usa o `IS` **nominal** do modelo × `AREA·M` e a
    temperatura da instância sem o `DTEMP` do modelo.
11. `VBICtemp` descarta o `EBBE` mapeado (`p[104]`); o load recebe o do
    cartão e o `fj` não o lê (recalcula `EBBEatT`).
12. `R_MIN`: `RCX = 0` **dado** vira 0,01 Ω e cria o nó interno; só o
    default (não dado) evita o nó.
13. `TNOM` em °C com default 27 fixo, independente de `.options tnom`.
14. `Irth` e `Qcth` escalam com `AREA·M` (a resistência térmica efetiva é
    `RTH/(AREA·M)`).
15. `vbicconv.c` lê as tensões de `CKTrhsOld` (o iterado antes da
    limitação) e devolve na primeira instância reprovada (só a contagem
    importa).

Reescrito, com motivo (linhas em `corpus/KNOWN_DIFFS.md`):

- **`Vrth` entre instâncias**: no C, `Vrth` é declarado uma vez no topo
  do `VBICload` (`= 0.0`) e só atribuído em instâncias com self-heating;
  uma instância sem self-heating carregada **depois** de uma com
  self-heating na mesma chamada herda o `Vrth` dela e roda aquecida.
  Medido no ngspice-47: um npn sem `RTH` a 2 V/0,75 V passa de 0,397 mA
  a 1,401 mA quando o cartão de um npn com self-heating (ΔT = 23,8 K)
  vem antes dele no deck. O Dune começa cada instância em 0 (load por
  dispositivo, independente da ordem; carga paralela). Nenhum deck do
  corpus monta essa ordem.
- `DEVlimitlog` com NaN zera sempre (o C, uma vez por processo).
- Bypass (`CKTbypass`, 0 por default no ngspice-47 também) e os valores
  `…hat` que só o alimentam: não portados.
- Nó térmico nomeado sem self-heating: diagonal unitária (0 V), o que o
  Sparse do ngspice entrega por nunca vê-lo.
- Constantes: `vt` do limitador e de `tVcrit` com `k/q` do 3f5 (só muda o
  caminho da limitação); o `fj` usa as constantes escritas no C.
- `.disto`/`.sens`: recusa (o C não tem as portas).

## Sondas (`corpus/regress/vbic_*.cir`, goldens do ngspice-47)

| Deck | Análise | Cobre |
|---|---|---|
| `vbic_gummel` | `.dc` | Gummel npn/pnp, todas as resistências, correntes ideais/não ideais, `WBE`, joelhos, Kull, parasita, `QBM=1`, cartão de 3 nós, `AREA`/`M` |
| `vbic_output` | `.dc` | famílias de saída npn/pnp: quase-saturação, avalanche, Early, `NKF`, `VRT`/`ART` |
| `vbic_parasitic` | `.dc` | saturação com o pnp parasita para o substrato, modo reverso (`ISRR`, `NR`, `IKR`), ruptura B-E |
| `vbic_temp` | `.dc` | `temp=85` com `TNOM=50`, todos os parâmetros de temperatura, `TEMP`/`DTEMP`/os dois, `DTEMP` do modelo, pnp a 125 °C |
| `vbic_tran` | `.tran` | inversores npn/pnp: depleções (`AJ* ≤ 0` e `> 0`), sobreposições, `QCO`, `CCSO`, `TF` dependente de polarização, `TR`, excesso de fase no npn |
| `vbic_uic` | `.tran` | `UIC` com `IC=`, `ICVBE`+`.ic`, `OFF` |
| `vbic_ac` | `.ac` | todas as capacitâncias do passo de pequenos sinais, `xf1`/`xf2`, `AJC`/`AJS > 0`, pnp com substrato a +2 V |
| `vbic_noise` | `.noise` | 7 térmicos (com `DTEMP`), 4 shot, 2 flicker, `M=2` |
| `vbic_pz` | `.pz` | npn emissor comum em pnp emissor comum, excesso de fase |
| `vbic_thermal_dc` | `.dc` | self-heating com nó térmico, expoentes de resistência, `SELFT=0` com nó nomeado |
| `vbic_thermal_tran` | `.tran` | `CTH` integrado, self-heating junto com excesso de fase |
| `vbic_thermal_ac` | `.ac` | polo térmico `RTH·CTH`, derivadas térmicas das cargas |
| `vbic_pnp_op` | `.op` | o ponto de operação errado do pnp com self-heating (Fidelidade 2) |

Transientes no perfil `spice3` (`.options compat=spice3`). Na régua de
precisão `vbic_tran` mede 0,043 e `vbic_thermal_tran` 0,157 (teto
default 0,2; as constantes de tempo do `vbic_tran` foram escolhidas para
o passo do deck as resolver); `vbic_uic` tem teto 0,4 em
`corpus/ACCURACY` (medido 0,316): o início UIC fixa só as tensões de
junção, os nós internos (`collCI`, `baseBI`) partem de 0 V e esse
transitório do primeiro passo não encolhe com o `TMAX`. Dois achados
fora do VBIC, de passagem: o `.pz` do Dune acha menos polos que o do
ngspice quando o circuito tem um segundo subcircuito desacoplado da
função de transferência (também com o Gummel-Poon; por isso o `vbic_pz`
liga os dois estágios); e o `.temp` é ignorado pelo Dune (cartão obsoleto
do 3f5), ao passo que o ngspice o aplica — os decks usam `.options temp`.

## Pendências

- Consultas de operação (`@q1[ic]`, `vbicask.c`) e checagem SOA.
- Parâmetros de instância como defaults no cartão `.model` (recusados).
- HICUM/L2 (`LEVEL=8`).
