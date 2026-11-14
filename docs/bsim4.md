# BSIM4 4.8.3 — o MOSFET `LEVEL=14|54` do ngspice-47

Porte do BSIM4 4.8.3 de `src/spicelib/devices/bsim4/` do ngspice-47
(Fase 8, item 8; D-0060). Licença dos arquivos traduzidos: **Educational
Community License 2.0** — "Copyright (c) 2025 University of California;
Project Directors: Prof. Sayeef Salahuddin and Prof. Chenming Hu;
Developers list: https://www.bsim.berkeley.edu/models/bsim4/auth_bsim4/".
Todo arquivo Rust traduzido reproduz esse cabeçalho e declara que é uma
tradução modificada (exigência da ECL-2.0). As versões 4.5–4.7
(`bsim4v5/6/7`, com o `B4TERMS_OF_USE`) **não** foram abertas.

O 3f5 não tem BSIM4: contrato e oráculo são o ngspice-47. Tudo fica atrás
da feature `bsim` (seção "Feature `bsim`" de `docs/dune-core.md`).

Código: `crates/dune-core/src/bsim4/`; elaboração em `elaborate.rs`
(`bsim4_model`, `bsim4_instance`) e `circuit.rs` (`Spec::Bsim4`,
`bsim4_setup`, passos 1 e 2 do `build`).

## Como foi traduzido

O C do BSIM4 é muito regular (atribuições, `if`/`else`, `switch`, acessos
`model->`, `here->`, `pParam->`). A tradução foi **mecânica, comando a
comando**: um tradutor de um subconjunto de C para Rust (fora do repositório,
como as sondas) fez a análise sintática dos arquivos do ngspice e emitiu o
Rust com:

- os campos das três estruturas do `bsim4def.h` com o nome do C em snake
  case (`BSIM4SjctTempSatCurDensity` → `sjct_temp_sat_cur_density`,
  `BSIM4capMod` → `cap_mod`), as flags `…Given` em `given.*`;
- as variáveis locais do C com o nome em minúsculas (colisões como `Vgs`
  × `vgs` ganham `_`: `vgs_`), declaradas no topo como no C;
- coerção de tipos explícita (`int` × `double` como no C: inteiro vira
  `f64` só onde o C promove), operadores compostos onde o C tem `x = x op y`
  (mesmo arredondamento), `if` aninhado sem `else` fundido em `&&`
  (mesma semântica de curto-circuito);
- `MAX`/`MIN` com a semântica da macro, `DEXP` de 2 e 3 argumentos, `pow`,
  `log`, `exp`, `sqrt` da libm;
- estados `*(ckt->CKTstate0 + here->BSIM4x)` → `ctx.states`, estampas
  `*(here->BSIM4XyPtr) += v` → `ctx.add`, RHS → `ctx.add_rhs`, AC `+1`
  (parte imaginária) → `add_b`.

Os `goto` do `b4ld.c` (`finished`, `line850`, `line860`, `line900`,
`line1000`; `line755` era só do bypass) viraram as variáveis
`goto_finished` e `path` com a mesma ordem de execução. Depois da tradução,
o código passou pelo `rustfmt`/`clippy` e por ajustes pontuais (cada um
equivalente: `clamp` no `vbsc`, um `||` no lugar de dois ramos idênticos do
`Xjbv*`). A verificação é o oráculo: os 20 decks `bsim4_*` batem com o
ngspice-47 com margem ≤ 0,006 (a maioria 0,000), e as derivadas foram
conferidas por diferenças finitas.

## Mapa C → Rust

| C (ngspice-47, linhas) | Rust (linhas) | Conteúdo |
|---|---|---|
| `bsim4def.h` (3931), `b4.c` (1087), `b4mpar.c` (3685), `b4par.c` (240) | `params.rs` (8119) | `Bsim4Model` (929 campos + 893 flags), `Bsim4Size`, `Bsim4Here`, os setters `BSIM4mParam`/`BSIM4param` gerados, listas de palavras-chave |
| `b4set.c` (2779) | `setup.rs` (3167) | padrões do modelo e da instância, regra de criação dos nós internos (`internal_nodes`, `assign_nodes`); o `TSTALLOC` está em `mod.rs` (`Device::alloc`) |
| `b4temp.c` (2408) | `temp.rs` (1797) + `Bsim4Knots` em `mod.rs` | as três partes do `BSIM4temp` (modelo, nó de tamanho, instância) e `BSIM4DioIjthVjmEval` |
| `b4geo.c` (390) | `geo.rs` (476) | `BSIM4PAeffGeo`, `BSIM4RdseffGeo`, `BSIM4RdsEndIso/Sha`, `BSIM4NumFingerDiff` |
| `b4check.c` (935) | `check.rs` (733) | `BSIM4checkModel` |
| `b4ld.c` (5623) | `load.rs` (4923) | `BSIM4load` inteiro (sem bypass nem OpenMP) e `BSIM4polyDepletion` |
| `b4acld.c` (684) | `acld.rs` (765) | `BSIM4acLoad`, com o NQS de AC |
| `b4pzld.c` (769) | `pzld.rs` (859) | `BSIM4pzLoad` |
| `b4noi.c` (679) | `noise.rs` (286) | `BSIM4noise`, ramo `N_DENS` (escrito à mão: `NevalSrc` → `NoiseCtx`) |
| `b4cvtest.c` (208), `b4trunc.c` (71), `b4getic.c` (57) | `mod.rs` (879) | `conv_test`, `trunc`, `set_ic` |
| — | `tests.rs` (353) | diferenças finitas, política de versão, tabela de parâmetros |

Não portados (não fazem parte do motor): `b4ask.c`/`b4mask.c` (consulta
de parâmetros `@m1[...]`), `b4mdel.c`, `b4soachk.c` (aviso de área de
operação segura), `b4bindCSC.c` (KLU), `bsim4init.c`.

## Seletores do modelo

Todos os valores aceitos pelo `b4set.c` 4.8.3 estão portados (o `b4ld.c`
foi traduzido inteiro) e cada linha da tabela tem deck no oráculo.

| Seletor | Valores portados | Deck | Recusado |
|---|---|---|---|
| `mobmod` | 0–6 | `bsim4_idvg` (0), `bsim4_modes_dc` (1–6) | — |
| `capmod` | 0, 1, 2 | `bsim4_ac` (2), `bsim4_modes_ac` (0, 1), `bsim4_nqs` (0, 1) | — |
| `cvchargemod` | 0, 1 | `bsim4_modes_ac` | — |
| `diomod` | 0, 1, 2 | `bsim4_body` (1), `bsim4_modes_dc` (0, 2 + TAT) | — |
| `rdsmod` | 0, 1 | `bsim4_modes_dc`, `bsim4_modes_ac` | — |
| `rbodymod` | 0, 1, 2 | `bsim4_rbody` | — |
| `rgatemod` | 0, 1, 2, 3 | `bsim4_rgate` | — |
| `permod` | 0, 1 | `bsim4_modes_dc` | — |
| `geomod` | 0–10 (instância e modelo) | `bsim4_nf` | fora de 0–10 (o `b4geo.c` não casa nenhum ramo e deixa lixo; o Dune recusa) |
| `rgeomod` | 0, 1 (instância) | `bsim4_nf` | — |
| `igcmod` | 0, 1, 2 | `bsim4_igc`, `bsim4_noise2` | — |
| `igbmod` | 0, 1 | `bsim4_igc` | — |
| `tempmod` | 0–3 | `bsim4_temp` (0), `bsim4_modes_dc` (1–3) | — |
| `fnoimod` | 0, 1 | `bsim4_noise2`, `bsim4_noise` | — |
| `tnoimod` | 0, 1 | `bsim4_noise`, `bsim4_noise2` | **2** (fontes correlacionadas do `NevalSrc2`, que o `NoiseCtx` não tem) |
| `trnqsmod` | 0, 1 | `bsim4_nqs` | — |
| `acnqsmod` | 0, 1 | `bsim4_modes_ac` | — |
| `mtrlmod` | 0, 1 | `bsim4_modes_dc` | — |
| `mtrlcompatmod` | 0, 1 | (sem deck) | — |
| `gidlmod` | 0, 1 | `bsim4_modes_dc` | — |
| `wpemod` | 0, 1 (com `SC`) | `bsim4_modes_dc` | — |
| stress (LOD) | `SA`/`SB`/`SD`, `SAREF`… | `bsim4_modes_dc` | — |
| `xpart` | 0, 0,5, 1 | `bsim4_modes_ac`, `bsim4_nqs` | — |
| binning | `l*/w*/p*`, `binunit` 1 e 2 | `bsim4_bin` | — |

Análises: `.op`, `.dc`, `.ac`, `.tran`, `.noise`, `.pz`, `.tf`. O C não
tem `.disto` nem `.sens` para o BSIM4: o Dune recusa as duas com o erro
estruturado de dispositivo sem suporte.

## Política de versão

`inpdomod.c` (BSD) escolhe o código pelo `VERSION`: 4.0–4.5 → `bsim4v5`,
4.6 → `bsim4v6`, 4.7 → `bsim4v7`, 4.8/`default` → `bsim4` (4.8.3);
qualquer outra → erro "Device type BSIM4 version X not available in this
binary". O Dune só tem o código 4.8.3:

| `VERSION` no cartão | ngspice-47 | Dune |
|---|---|---|
| 4.8, 4.8.0–4.8.3, "4.8x" | `bsim4` | nativo (`v48int_version` como o `b4set.c`) |
| ausente | `bsim4` com aviso "Version not specified" | 4.8.3, com nota |
| 4.0–4.7 | `bsim4v5`/`v6`/`v7` | **equações 4.8.3**, uma nota por família de modelo (`nmos_3p3.0…15` = uma nota); internamente tratado como 4.8.3 (`v48int_version = 483`, sem o aviso do `b4set.c`, sem os testes da 4.7 do `b4check.c`) |
| outro (3.3, 5.0, …) | erro | o mesmo erro, quando instanciado |

**Decisão para o dono** (implementada como pedido): a escolha de rodar
4.5–4.7 nas equações 4.8.3. Medido no inversor gf180 como vem (4.5) contra
o ngspice rodando o `bsim4v5`: DC máx. 7,1e-4 A em `i(vdd)` (margem 0,669),
transiente máx. 0,16 mV em `v(out)`; `docs/pdk-parser.md` já tinha medido
≤ 0,56 mV no sky130. O deck `bsim4_v45` fixa o caminho da nota; o golden
dele foi gerado pelo ngspice com o mesmo deck reescrito para 4.8.3.

## Parâmetros

`params.rs` é gerado das tabelas `BSIM4mPTable`/`BSIM4pTable` do `b4.c`
(nomes e aliases) e dos `case` do `b4mpar.c`/`b4par.c` (inclusive os
ajustes: `tnom` em °C, `ndep`/`nsd`/`ngate` e coeficientes de binning acima
de 1e20/1e23 lidos em cm⁻³). Os padrões são os do `b4set.c`, traduzido
inteiro em `setup.rs`. Inteiros arredondam como o `INPgetValue` do ngspice
(`floor(0,5 + v)`). Parâmetro de modelo desconhecido é aviso (como o
`INPmakeMod`); de instância é erro. `dtemp` é aceito e não tem efeito (o
4.8.3 não o lê; nota). Não há `.option scale` no Dune: `l`/`w`/`ad`… não
são escalados.

## Nós internos

`BSIM4setup` cria, nesta ordem e com estes nomes: `#drain`, `#source`
(RDSMOD 1, ou RSH com NRD/NRS dados > 0, ou RGEOMOD com `Rtot > 0`, ou
TNOIMOD 1 com análise `.noise` no deck), `#gate` (RGATEMOD > 0),
`#midgate` (RGATEMOD 3), `#dbody`, `#body`, `#sbody` (RBODYMOD 1|2) e
`#charge` (TRNQSMOD 1). Os vetores do rawfile batem com os do ngspice
(`v(mn1#charge)` no `bsim4_nqs`). O `noiseAnalGiven` do C (o C procura um
job de ruído na tarefa) é `Circuit::noise_job`, ligado pela elaboração
quando o deck tem `.noise` e sempre pela análise de ruído.

## Fidelidade: o que o C faz e o Dune repete

Portado literalmente (cada item conferido no C):

1. **`u0temp` sobrescrito** (`b4temp.c:1763`): `here->BSIM4u0temp =
   pParam->BSIM4u0temp * here->BSIM4mulu0` vem *depois* do efeito de stress
   (`:1681`) e do WPE (`:1755`) e descarta as duas correções de mobilidade
   (o `mulu0` do ngspice foi acrescentado por cima). O Dune repete: com SA/SB
   ou WPEMOD só o Vth/k2/eta0/vsat mudam. Provável bug do ngspice.
2. **`Lnew` velho na resistência de porta** (`b4temp.c:1929`): `grgeltd`
   usa o `Lnew` do último nó de tamanho *construído*, não o da instância;
   duas instâncias que reusam nós diferentes herdam o `L` da outra. O
   `TempVars` do Dune carrega os locais do `BSIM4temp` entre instâncias como
   o C, então repete. Provável bug.
3. **Nomes trocados nas mensagens** (`b4temp.c:480…`): "mosfet %s, model
   %s" recebe o nome do modelo e depois o da instância. Repetido.
4. **Mensagem errada** (`b4check.c:122`): o teste de `ni0sub` imprime
   "Easub = …". Repetido.
5. **O `b4check.c` altera o modelo e o nó de tamanho** (`cgdo/cgso/cgbo` < 0
   → 0, `a2`, `rdsw`, `ckappas`…) só na primeira instância de cada modelo,
   depois do nó dela já calculado; as instâncias seguintes veem o modelo
   alterado. O Dune repete (a verificação roda com o nó recém-construído
   mutável).
6. **Derivadas aproximadas do Igb** (`b4ld.c:2819` e o ramo do Igbinv):
   `Vaux` usa `Vbseff`, mas `dVaux_dVb` supõe `dVbseff/dVb = 1`. O Jacobiano
   estampado difere da diferença finita em até 9,1e-4 da escala da linha
   (teste `jacobian_igb_approximate_like_the_c`). Repetido.
7. **`T0 *= T0 * 4.0e-16`** (`b4ld.c:1342`): quadrado escrito como operador
   composto. O Rust faz a mesma conta por um temporário (o clippy recusa a
   forma literal).
8. **Locais que atravessam as partes do `BSIM4temp`**: `Ldrn`/`Wdrn` são
   atribuídos antes do bloco do nó de tamanho e usados nele e na parte da
   instância; o Dune atribui nos dois pontos, como o C os vê.

## Desvios deliberados (também em `corpus/KNOWN_DIFFS.md`)

- **Sem bypass** (`NOBYPASS`), como todo dispositivo do Dune; o
  `CKTbypass` do ngspice está ligado por padrão, mas o oráculo bate.
- **Sem estado global**: o "verifique o modelo uma vez" do `b4check.c` usa
  um `static char modname[]` que vale para o processo inteiro (uma segunda
  simulação do mesmo modelo não verifica de novo); no Dune vale por
  construção do circuito.
- **Constantes do 3f5** (`CHARGE`, `CONSTboltz`, `CONSTvt0`, D-0002) no
  `BSIM4polyDepletion`, no `DEVpnjlim` e no ruído; os literais próprios do
  BSIM4 (`KboQ`, `Charge_q`, `EPS0`, `EPSSI`, `PI = 3.141592654`) como estão.
- **Mensagens** do `b4set.c`/`b4temp.c`/`b4check.c` viram notas (o C
  imprime em stdout/stderr e escreve `bsim4.out`).
- **Ruído FNOIMOD 1**: Ssi e Swi escalam ambos com f^-EF, então a
  combinação é avaliada em 1 Hz e entregue como lei de potência `coef/f^EF`
  (assim o ruído transiente do Dune também a usa). Diferença de
  arredondamento só.
- **TNOIMOD 2 e GEOMOD fora de 0–10 recusados** com mensagem.
- Versões 4.0–4.7 nas equações 4.8.3 (acima).

## Decks (`corpus/regress/bsim4_*.cir`)

Cartões escritos para o Dune (classe 0,18 µm, valores próprios, nenhum
copiado de PDK), `version=4.8.3` explícito, uma análise por deck, goldens
do ngspice-47. Transientes com `.options compat=spice3`.

| Deck | Análise | Pior margem | Cobre |
|---|---|---|---|
| `bsim4_idvg` | dc | 0,000 | n e p, linear/saturação, efeito de corpo |
| `bsim4_idvd` | dc | 0,000 | famílias de saída, modo inverso (Vds < 0), GIDL, L longo |
| `bsim4_body` | dc | 0,000 | varredura de corpo, junções diretas, IJTH |
| `bsim4_nf` | dc | 0,000 | NF 2–4, GEOMOD 0/1/2/4/9/10, MIN, RGEOMOD 1 |
| `bsim4_temp` | dc | 0,000 | 85 °C, TEMPMOD 0 |
| `bsim4_bin` | dc | 0,000 | binning, BINUNIT 1 e 2 |
| `bsim4_mult` | dc | 0,000 | M |
| `bsim4_igc` | dc | 0,000 | IGCMOD 1/2, IGBMOD 1, n e p |
| `bsim4_modes_dc` | dc | 0,000 | MOBMOD 1–6, RDSMOD 1, TEMPMOD 1–3, DIOMOD 0/2, GIDLMOD 1, MTRLMOD 1, WPE, LOD, PERMOD 0 |
| `bsim4_v45` | dc | 0,000 | VERSION 4.5 → nota |
| `bsim4_ac` | ac | 0,000 | CAPMOD 2, overlaps, junções |
| `bsim4_modes_ac` | ac | 0,000 | CAPMOD 0/1, CVCHARGEMOD 1, ACNQSMOD 1, RDSMOD 1, XPART |
| `bsim4_rgate` | ac | 0,000 | RGATEMOD 1/2/3 |
| `bsim4_rbody` | ac | 0,000 | RBODYMOD 1 e 2 |
| `bsim4_pz` | pz | 0,000 | `BSIM4pzLoad` |
| `bsim4_noise` | noise | 0,004 | FNOIMOD 1, TNOIMOD 0, ruído de RSH |
| `bsim4_noise2` | noise | 0,002 | FNOIMOD 0, TNOIMOD 1 (nós S/D pela análise de ruído), ruído de disparo da porta |
| `bsim4_inv` | tran | 0,001 | inversor com carga |
| `bsim4_ring` | tran | 0,000 | três estágios de anel abertos (um anel livre acumula fase e não é refinável pela régua) |
| `bsim4_nqs` | tran | 0,005 | TRNQSMOD 1 (nó `#charge`), CAPMOD 0/1 |

Régua de precisão: `bsim4_inv` 0,002, `bsim4_ring` 0,001, `bsim4_nqs`
0,002 (teto padrão 0,2).

## Validação com os PDKs (sem copiar arquivos)

Bancadas do levantamento (`.claude/scratch/pdk/work/tb/ver_*`, `version`
reescrito para 4.8.3), ngspice-47 contra Dune. O parser ainda não lê
parâmetros de subcircuito, então as sondas (fora do repositório, em
`.claude/scratch/bsim4-probes/`) tiram os blocos `.subckt` não usados e
resolvem o bin (gf180, cartões de topo) ou extraem os dois cartões que o
inversor usa com números literais (sky130: parâmetros globais avaliados,
`scale=1u` aplicado à mão, `ad=0 … sd=0` dados como no subcircuito; o
ngspice no deck extraído reproduz o deck completo a 5e-15).

| Inversor | Análise | Máx. desvio | Margem |
|---|---|---|---|
| gf180 (bins `nmos_3p3.4`/`pmos_3p3.8`) | dc 0–3,3 V | 6,9e-4 V em `v(out)` | 0,148 |
| gf180 | tran 10 ns (`compat=spice3`) | 6,5e-7 V em `v(out)` | 0,000 |
| gf180 como vem (4.5) × `bsim4v5` | dc / tran | 7,1e-4 A em `i(vdd)` / 1,6e-4 V | 0,669 / 0,000 |
| sky130 (RBODYMOD 1, TNOIMOD 1) | dc 0–1,8 V | 1,0e-3 V em `v(out)` | 0,319 |
| sky130 | tran 10 ns | 1,5e-3 V em `v(mn#dbody)` | 0,085 |

No DC o ponto de operação isolado bate a 1e-12; o desvio vem da varredura
na região de ganho alto (o Newton para em pontos diferentes dentro do
`reltol`).

## Pendências

- TNOIMOD 2 (`NevalSrc2` no `NoiseCtx`, para `.noise`, `.sp` e o ruído
  transiente).
- `.option scale`, parâmetros de subcircuito e modelo por instância (do
  parser, não do BSIM4) para ler o sky130 sem extração.
- `@m1[gm]`-style (`b4ask.c`) não existe no Dune para nenhum dispositivo.
