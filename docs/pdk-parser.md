# Parser para PDKs — medição (Fase 8, item 7)

Medição, não implementação: o que falta ao `dune-netlist` (parser) e ao
`dune-core::elaborate` para ler as bibliotecas de modelos ngspice de três
PDKs abertos, e qual deles é o primeiro alvo barato antes do BSIM4
(item 8, D-0060). Feito em 2026-11-14 sobre `ngspice-leva` (`0a3e407`).
Nenhum código foi alterado.

**Regra de proveniência.** A semântica de `.param`/`{…}` abaixo foi obtida
**só** do comportamento observado do `ngspice` 47 (Homebrew) em decks de
sonda (Apêndice A) e do manual; nada sob `src/frontend/numparam` (LGPL) foi
aberto. Trechos do ngspice citados vêm de arquivos Modified BSD
(`spicelib/parser/inpgmod.c`, `inpdomod.c`, `misc/string.c`) ou ECL-2.0
(`devices/bsim4`).

## 0. Resumo

1. **Hoje o Dune não lê nenhum dos três**: os três testbenches param na
   linha 2 (`.lib` não suportado). Achatando as bibliotecas à mão
   (ferramenta de medição, §2), o parser emite 4362 diagnósticos no
   sky130, 91 no gf180 e 78 no IHP, e nunca chega à elaboração.
2. **gf180 é o primeiro alvo barato**: o inversor usa `M` direto num
   modelo binado de topo (`nmos_3p3.0…15`), sem subcircuito invólucro, sem
   `.option scale`, 7 parâmetros globais no fecho; precisa de `.lib`,
   expressões entre aspas e binning. sky130 exige tudo isso **mais** escopo
   de parâmetros de subcircuito, modelo por instância e `scale`.
3. **IHP sg13g2 não é alvo de BSIM4**: os MOS são PSP 103.6 via OSDI
   (Verilog-A compilado). Nem o ngspice roda o inversor sem os `.osdi`.
4. **Versão do BSIM4**: sky130 e gf180 declaram `version = 4.5`, que o
   ngspice-47 encaminha para `bsim4v5` (com o `B4TERMS_OF_USE`, fora por
   D-0060). Medido: reescrevendo para `4.8.3` o inversor muda no máximo
   92 µV (DC) e 0,56 mV (transiente, numa borda de 1,74 V) — dentro de
   `reltol`. 4.6, 4.7 e 4.8.3 dão números idênticos entre si. Um porte da
   4.8.3 pode ser oráculo-comparado **bit a bit** com o ngspice rodando o PDK
   com `version` reescrito, e dentro da tolerância com o PDK como vem.
5. **Nenhum travamento nem pânico** do parser atual em 869 arquivos reais
   (+ 57 MB sintéticos); o maior arquivo (1,36 MB) lê em 0,05 s.
6. Dois desvios do Dune atual frente ao ngspice-47 achados de passagem
   (§8): `{2^3^2}` vale 512 no Dune e 64 no ngspice; `.include "arq"` com
   aspas não abre. Ambos corrigidos em 2026-11-14, junto com a G1.

## 1. Fontes

Tudo em `/Users/leite/Github/dune/.claude/scratch/pdk/` (gitignorado,
durável), baixado como tarball (`codeload`/release, sem histórico), com
user-agent genérico. Só leitura em tempo de execução; nada redistribuído.

| PDK | Origem | Commit / versão | Licença | Em disco |
|---|---|---|---|---|
| sky130 (bruto) | `github.com/google/skywater-pdk-libs-sky130_fd_pr` (`models/`, `cells/**.spice`) | `f62031a1be9aefe902d6d54cddd6f59b57627436` | Apache-2.0 (`LICENSE`; cabeçalho Apache em cada `.spice`, "The SkyWater PDK Authors") | 52 MB |
| sky130A (open_pdks) | `github.com/fossi-foundation/ciel-releases`, release `sky130-e7a6777d…` (`common.tar.zst` + `sky130_fd_pr.tar.zst`; extraídos `libs.tech/ngspice` e `libs.ref/sky130_fd_pr/spice`) | open_pdks `e7a6777d157a9d94c7ac757457b9e4309e9e9000` (1.0.619), `sky130_fd_pr` `29b55153fd9f4badef05a02667184b81789d1f61` | Apache-2.0 (cabeçalhos dos arquivos; o tarball não traz `LICENSE` próprio) | 72 MB (20 MB comprimido) |
| gf180mcu | `github.com/google/globalfoundries-pdk-libs-gf180mcu_fd_pr` (`models/ngspice/`) | `9f992d5a9186d1f7820c58f039c484ad35b2edea` | Apache-2.0 (`LICENSE`) | 67 MB (65 MB são `testing/`) |
| IHP sg13g2 | `github.com/IHP-GmbH/IHP-Open-PDK` (`ihp-sg13g2/libs.tech/ngspice/`, `verilog-a/`) | `e73adb4045731ddec21634146d536380057f6a26` | Apache-2.0 (`LICENSE`; cabeçalho Apache "IHP PDK Authors" nos `.lib`). Os `.va` de terceiros (PSP, VBIC, r3_cmc) trazem licenças próprias — não lidos, irrelevantes aqui | 1,3 MB |

Total 192 MB de PDK + 83 MB de artefatos de medição (`work/`). O sky130
**bruto** não roda no ngspice-47 (`Not enough parameters for i source` em
`sky130_fd_pr__nfet_05v0_nvt.pm3.spice:820`, uma linha `include` sem
ponto); o open_pdks corrige. Toda a medição de sky130 abaixo usa o
**sky130A do open_pdks**, que é o que xschem e afins usam.

Bibliotecas medidas ("biblioteca completa"):

| PDK | Arquivos | Linhas físicas | Maior arquivo |
|---|---|---|---|
| sky130A | 821 `.spice` (52 MB) | 1 854 449 | 1 361 804 B (`…pfet_01v8_hvt__sf.pm3.spice`) |
| gf180 | 3 `.ngspice` (1,3 MB) | 48 293 | 1 348 553 B (`sm141064.ngspice`) |
| IHP | 38 `.lib` (488 KB) | 7 899 | 94 408 B (`sg13g2_moslv_parm.lib`) |

## 2. Método

Artefatos em `.claude/scratch/pdk/work/` (`bin/`, `tb/`, `census/`):

- `tb/<pdk>/inv.cir` (op+dc+tran, para o Dune), `inv_dc.cir` e
  `inv_tran.cir` (para o ngspice: o ngspice-47 em batch recusa um deck com
  `.dc` **e** `.tran` impressos — "no data saved for Transient analysis",
  reproduzido com um RC de 4 linhas). Inversor: PMOS w=2 µm, NMOS w=1 µm,
  L mínimo, carga 10 fF, `.dc vin 0 VDD 0.01`, `.tran 10p 10n`.
- `bin/pdkflat.py`: achata `.lib arquivo seção`/`.include` (aspas, caminho
  relativo ao arquivo que inclui, diretivas indentadas) num deck único, e
  lista arquivo×seção alcançados (`reach.txt`). Instrumento de medição, não
  proposta de implementação.
- `bin/census.py`: censo por construção sobre cartões lógicos
  (continuações juntadas, comentários cortados). Dois universos: **inv** =
  os arquivo×seção que o testbench alcança; **full** = todos os arquivos da
  biblioteca (sky130 conta todos os cantos, então repete).
- `bin/closure.py`: o que o inversor **avalia** — fecho a partir dos `X`/`M`
  do topo: subcircuitos, todos os bins dos modelos usados e os `.param`
  globais referenciados (transitivamente).
- Tempos e memória: `/usr/bin/time -l` (wall, RSS máximo). Dune em
  `--release` (`CARGO_TARGET_DIR=~/.cargo/agent-target-four`).

## 3. Inversor no ngspice-47 e no Dune

| PDK | Inclusão | ngspice-47 | Dune hoje |
|---|---|---|---|
| sky130A | `.lib ".../sky130.lib.spice" tt`; `X… sky130_fd_pr__{n,p}fet_01v8 w= l=` (µm, `.option scale=1.0u` da biblioteca) | roda; 16,3 s (dc) / 17,2 s (tran) wall, 417 MB RSS; Vout(Vin=0,9 V)=0,4989 V. O mesmo deck achatado roda em 1,7 s (o custo é o `.lib`). Sem avisos (só a tabela de parâmetros não consultáveis `<<NAN, error = 7>>` do `.op`) | para na linha 2: `.lib is not supported yet`; `X` com `w=… l=…` vira "no such subcircuit `0.15`" |
| gf180 | `.include ".../design.ngspice"` + `.lib ".../sm141064.ngspice" typical`; `M… nmos_3p3/pmos_3p3 w= l=` | roda; 0,09 s, 18 MB; Vout(0,9 V)=3,2103 V | `.include` com aspas não abre (procura `…/"/Users/…"`), depois `.lib` |
| IHP | `.lib ".../cornerMOSlv.lib" mos_tt`; `X… sg13_lv_{n,p}mos w= l= ng= m=` | **não roda**: `Unknown model type psp103va - ignored`, `Unable to find definition of model xp:sg13g2_lv_pmos_psp` (PSP via OSDI; os `.osdi` não estão no repositório e não foram compilados nem baixados — §10) | `.lib`, depois `X` com parâmetros |

Modo de compatibilidade: o `spinit` do sky130 liga `ngbehavior=hsa`; o
`.spiceinit` dos testes do gf180 liga `hs`. Medido com e sem: diferença
máxima no DC de 1,0 mV (sky130, Vin=0,81 V) e 0,69 mV (gf180), da ordem de
`reltol·|V|`; o transiente do sky130 coincide (1e-12 V). Repetições do
mesmo deck são determinísticas.

Achatado à mão (`pdkflat.py`) e passado ao Dune, os diagnósticos por classe:

| Classe | sky130 (7,1 MB, 265 522 linhas) | gf180 (237 KB) | IHP (391 KB) |
|---|---|---|---|
| `malformed parameter expression` (espaço/parêntese/vírgula dentro de `{}`) | 3628 | — | — |
| `unknown parameter` (funções, escopo de subcircuito, `temper`) | 475 | 1 | — |
| `unexpected ' in expression` (expressão entre aspas) | 177 | 72 | — |
| `` `X` cannot be a parameter name `` (pedaços de expressão cortada) | 72 | 17 | — |
| `no such subcircuit` (parâmetros na linha `X`) | 2 | — | 2 |
| `unknown model type psp103va/pspnqs103va` | — | — | 24 |
| `unknown device type nsg13_lv_nmos` (letra `N`, OSDI) | — | — | 16 |
| `.if/.else/.endif is not allowed inside .subckt` | — | — | 36 |

O Dune leu os 7,1 MB em 0,16 s com 88 MB de RSS e parou antes da
elaboração (que nem chega a ver `LEVEL=54`).

## 4. Censo

Contagens de ocorrências em cartões lógicos (`census.py`), "inv" = alcance
do testbench, "full" = biblioteca inteira.

### 4.1 Estrutura de arquivo e léxico

| Construção | sky130 inv / full | gf180 inv / full | IHP inv / full | Dune hoje |
|---|---|---|---|---|
| arquivo×seção alcançados | 170 | 9 (2 arquivos; `sm141064.ngspice` relido 8×) | 3 (6 splices: o `…_parm.lib` é incluído **dentro** de cada `.subckt`) | — |
| `.lib`/`.endl` (definições+chamadas) | 1 chamada / 51+51 | 7 / 144+59 | 1 / 51+51 | **não** |
| `.include` | 169 / 1623 | 0 / 0 | 5 / 109 | só coluna 1, sem aspas, sem `.inc` |
| caminho entre aspas (`"…"`/`'…'`) | sim | sim | não | **não** |
| diretiva indentada (`  .lib`, `  .param`) | 0 / 6 | 30 / 166 | 77 / 1435 | `.include` indentado é erro |
| continuação `+` (indentada) | 237 442 (1) / 1 673 044 (1) | 7751 (15) / 45 734 (156) | 888 (116) / 3161 (464) | sim |
| comentário `*` de linha | 24 608 / 167 799 | 152 / 1030 | 143 / 1566 | sim |
| comentário `;` de fim de linha | 622 / 862 | 0 | 0 / 9 | sim |
| `$` de fim de linha, `//` | 0 | 0 | 0 | — |
| cartão-ponto em maiúsculas (`.LIB`, `.PARAM`) | 4 / 5 | 0 / 66 | 0 / 112 | sim (case-insensitive) |
| `.control` | 0 | 0 | 0 | — |
| `.func` | 0 | 0 | 0 | — |
| `.params` (sinônimo) | 0 | 0 | 0 / 41 | **não** |
| maior linha física | 298 | 148 / 264 | 210 / 621 | ok |
| maior cartão lógico (bytes) | 344 942 / 1 260 574 (`parameters/montecarlo.spice`, um `.param` com milhares de `+`) | 8204 | 15 155 | ok (medido, §7) |
| maior expressão `{…}`/`'…'` | 285 | 79 / 208 | 79 / 613 | limite atual 512 — **IHP excede** |

### 4.2 Parâmetros e expressões

| Construção | sky130 inv / full | gf180 inv / full | IHP inv / full | Dune hoje |
|---|---|---|---|---|
| cartões `.param` | 700 / 1870 | 48 / 281 | 37 / 1901 | sim |
| definições `.param` | 14 060 / 170 406 | 131 / 1234 | 37 / 2066 | só número/`{a+b}` sem espaço |
| … com expressão (não número) | 509 / 14 755 | 78 / 542 | 0 / 459 | parcial |
| … expressão nua sem `{}`/`''` (`a=b*2`, `x=agauss(…)`) | 24 / 48 | 16 / 209 | 0 / 179 | só sem parêntese/vírgula |
| … com `agauss/gauss/aunif/unif` | 60 / 145 | 16 / 129 | 0 / 200 | **não** |
| `.param` **dentro** de `.subckt` | 323 / 454 | 24 / 115 | 0 / 195 | **não** ("not allowed inside .subckt") |
| nomes redefinidos | 193 / 20 083 | 11 / 229 | 0 / 251 | último vence (= ngspice) |
| referências adiante entre `.param` globais | 0 (no fecho e no alcance) | 0 | 0 | não suportado |
| `{…}` em `.model` | 10 649 / 64 442 | 0 | 0 | **não** |
| `'…'` em `.model` | 250 / 283 | 49 / 223 | 156 / 544 | **não** |
| nome nu de parâmetro como valor de `.model` (`noia = nmos_3p3_noia`) | 3 / 3 | 103 / 825 | 0 / 41 | **não** ("needs a numeric value") |
| `{}`/`''` em linha `M` | 567 / 1391 | 16 / 49 | — | só `{a+b}` simples |
| `{}`/`''` em linha `X` | 263 / 431 | 16 / 60 | 0 / 4 | **não** |
| `{}`/`''` em `R`/`C`/`D` | 720 / 965 | 12 / 157 | 0 / 33 | só simples |
| `{}`/`''` em linha `N` (OSDI) | — | — | 104 / 509 | — |

Funções usadas (ocorrências, todo o texto de expressão):

| Função | sky130 full | gf180 full | IHP full | no fecho do inversor |
|---|---|---|---|---|
| `agauss` | 23 168 | 129 | 188 | sky130: 972 (todas `MC_MM_SWITCH*AGAUSS(…)`, `mc_mm_switch=0` no `tt`) |
| `sqrt` | 23 121 | 27 | 124 | sky130: 972 |
| `gauss` | 0 | 0 | 129 | — |
| `max` / `min` | 7 / 32 | 12 / 4 | 184 / 0 | IHP: 44 `max` |
| `abs` | 53 | 108 | 0 | — |
| `pow` / `pwr` | 29 / 13 | 2 / 0 | 4 / 0 | — |
| `exp`, `log`, `ln` | 13, 5, 0 | 6, 0, 0 | 0, 4, 1 | — |
| `tanh`, `cosh` | 6, 6 | 16, 0 | 0, 0 | — |
| `floor` | 1 | 0 | 32 em 16 linhas `.if` (contado por `grep`; o censo não lê condições) | IHP `.if` |
| `v(a,b)` (resistor comportamental) | 104 | 149 | 6 | — |

Operadores e variáveis: `?:` sky130 5 / gf180 30 / IHP 192; `==` 0 / 72 / 0;
`!=` 0 / 0 / 182; `<=` `>=` `<` `>` poucos; `||` gf180 4; `**` 1 / 2 / 163;
`^` 0; `temper` 76 / 41 / 6. O fecho do inversor gf180 não usa função
nenhuma; o do sky130 só `agauss` e `sqrt`; o do IHP só `max`.

### 4.3 Subcircuitos

| Construção | sky130 inv / full | gf180 inv / full | IHP inv / full | Dune hoje |
|---|---|---|---|---|
| `.subckt` | 188 / 456 | 13 / 81 | 4 / 50 | sim, sem parâmetros |
| … com padrões na linha (`w=10u l=0.28u …`) | 32 (160 padrões) / 33 | 13 (205) / 81 (964) | 4 (52) / 20 (209) | **não** (vira porta) |
| … com `params:` | 0 | 0 | 0 | — |
| padrão via `.param` no corpo (padrão sky130) | sim (`sky130_fd_pr__nfet_01v8`: `.param l=1 w=1 ad=0 … mult=1 nf=1.0`) | — | — | **não** |
| `X` com `nome=valor` | 117 / 689 | 8 / 30 | 0 / 2 | **não** (último token vira nome do subcircuito) |
| `.model` dentro de `.subckt` dependente de parâmetro da instância | sim: os 288 bins do inversor (`toxe = {…/sqrt(l*w*mult)}`) | não | sim (IHP) | modelo local sim, dependente **não** |
| `.if/.elseif/.else/.endif` | 0 | 0 | 36 / 177 (todos em `.subckt`, por parâmetro da instância) | **não** |
| letras de elemento (full) | x 780, r 513, c 345, m 128, d 58, q 6, l 6 | r 70, c 37, x 30, d 25, m 23, q 10 | n 69, c 42, r 39, d 30, l 10, q 8, x 2 | `N` não existe |

Parâmetros nas linhas `X` (sky130 inv): `l` 116, `mult` 113, `ad/as/pd/ps`
109, `nrd/nrs` 108, `m` 107, `w` 58, `sa/sb` 20, `nf` 19, `sd` 18.

### 4.4 Modelos

| Tipo / nível / versão | sky130 full | gf180 full | IHP full | Dune hoje |
|---|---|---|---|---|
| `nmos/pmos level=54 version=4.5` (BSIM4 → `bsim4v5` no ngspice) | 6108 | 194 | — | **não** (item 8) |
| `level=54 version=4.6` (→ `bsim4v6`) | — | 6 | — | não |
| `level=54 version=4.62` (→ `bsim4v6`) | 1 (`pfet_20v0`) | — | — | não |
| `level=49 version=3.2` (BSIM3 → `bsim3v32`) | 14 (células especiais, SONOS) | — | — | só BSIM3v3.3 |
| `d level=3` (diodo com geometria) | 22 | 29 | 13 | diodo do 3f5 (`level` ignorado com nota) |
| `d level=1`/sem nível | — | — | 13 | sim (subconjunto de parâmetros) |
| `r` / `c` (modelo semicondutor) | 28 / 43 | 23 / 9 | 0 / 2 | parcial |
| `npn/pnp level=1` (Gummel–Poon) | 6 | 10 | 2 | sim |
| `npn level=9` (VBIC no ngspice) | — | — | 6 | não (item 4) |
| `psp103va`, `pspnqs103va`, `r3_cmc`, `mosvar`, `cap_cmomi/f` (OSDI) | — | — | 12, 3, 2, 2 | fora de escopo |

Binning (`.model base.N`, todos com `lmin lmax wmin wmax`):

| | sky130 | gf180 | IHP |
|---|---|---|---|
| grupos / bins (full) | 38 / 934 (máx. 198 por grupo) | 7 / 38 (máx. 16) | 0 |
| bins no fecho do inversor | 180 (n) + 108 (p) | 16 + 16 | — |
| `binunit` | 2 | 2 | — |

Parâmetros de instância MOS (linhas `M`, full): sky130 `l w ad as pd ps
nrd nrs` 128, `nf sa sb sd` 112, `m` 7, `delvto` 6; gf180 `w l as ad ps
pd nf nrd nrs sa sb sd` 23, `delvto` 19, `dtemp` 1. `mult` aparece só como
parâmetro de subcircuito (sky130).

`.option`: só `scale=1.0u` (sky130, em `all.spice`, alcançado pelo
inversor). Nenhum `scalm`, `wnflag`, `.temp`, `.control`.

## 5. Semântica do ngspice-47 observada

Decks e saídas no Apêndice A. "Dune" = comportamento atual.

| # | Pergunta | ngspice-47 | Dune |
|---|---|---|---|
| S1 | `.lib arq seção` | só o trecho `.lib seção`…`.endl`; nome de seção case-insensitive; `.lib` aninhado no mesmo arquivo funciona; `.include` dentro da seção resolve relativo ao **arquivo que inclui**; seção não pedida nem é lida (lixo nela não dá erro); `.endl nome` divergente aceito; pedir a mesma seção 2× é inofensivo; seção inexistente é fatal (P01, P21, P22, P24) | erro; **igual desde 2026-11-14** (G1; `.endl` divergente é aviso) |
| S2 | referência adiante entre `.param` | aceita (`a={b+1}` antes de `b=2` → 3) (P02) | erro; **igual desde 2026-11-14** (G2) |
| S3 | redefinição | a **última** definição vale para todos os usos, inclusive anteriores (P03) | igual (coleta tudo antes); desde a G2 também para dependentes (`d={c}` com `c` redefinido depois vale o último) |
| S4 | precedência em subcircuito | valor da linha `X` > padrão da linha `.subckt` = padrão via `.param` no corpo = `params:` > `.param` global de mesmo nome (sombreado) (P04, P18, P23) | erro; **igual desde 2026-11-14** (G3; atuais avaliadas no escopo do chamado, escopo dinâmico — sondas `s1`–`s3` em `docs/dune-netlist.md`) |
| S5 | `.param` local encadeado (`lw={w*g}`, `lw2={lw+1}`) | funciona, mistura global e da instância (P15) | erro; **igual desde 2026-11-14** (G3) |
| S6 | `{…}` × `'…'` × nu | equivalentes; espaços livres; nomes case-insensitive; nome nu de parâmetro vale em `.model` e na instância (`rsh=rv`, `w=wv`) (P05b, P11, P19) | erro; **igual desde 2026-11-14** (G2) no escopo global — nome nu só depois de `nome=` |
| S7 | precedência/associatividade | `^` e `**` **associam à esquerda** (`2^3^2 = 2**3**2 = 64`); menos unário mais fraco que `^` (`-2^2 = -4`) (P05, D_POW) | `^` à direita: 512; **igual desde 2026-11-14** (`**` desde a G2) |
| S8 | funções | `pwr(-2,3)=8` (não preserva sinal), `pow(-2,3)=-8`, `pow(-2,0.5)` e `sqrt(-4)` → NaN → erro fatal "unknown parameter (nan)" **se usado**; `log` é **natural** (`log(100)=4,6052`), `log10`; `nint(2.5)=2`; `int` trunca; `7/2=3,5`; `limit(5,0,3)=8` (não é clamp; nenhum PDK usa); `!0=1`; booleanos valem 0/1 (P05) | sem funções; **igual desde 2026-11-14** (G2: as 34 funções do ngspice-47; NaN usado é erro com span; aridade errada é erro, o ngspice a lê errado) |
| S9 | sufixos em expressão | `2k*3=6000`; **`1mil` = 1e-3** (não 25,4e-6 como no leitor de números do 3f5) (P05 e21) | `mil`=25,4e-6; **igual desde 2026-11-14** dentro de expressões (fora delas segue o `INPevaluate` do 3f5) |
| S10 | ternário em linha `X` entre aspas (`lr='(d_sab==0) ? 2 : d_sab'`) | funciona (P17) | erro |
| S11 | `agauss/gauss/aunif/unif` | **aleatórios a cada execução** sem semente fixa (duas rodadas: 0,9708 / 1,0276); `0*agauss(…)` = 0 (P06) | erro; desde a G2 lidas inteiras e avaliadas no **nominal** com um aviso por deck (decisão pendente, G8) |
| S12 | `temper` | vale em `.model`, na instância e em `.param` local de subcircuito, com a temperatura do circuito (`.options temp`/`.temp`); em `.param` **global** é erro fatal "Undefined parameter [temper]" (P07, P07a–d) | erro; desde a G2 erro "not supported yet" onde é avaliado (G7) |
| S13 | `.model` local dependente da instância | uma cópia por instância (`rsh={w*100}` → 100 e 300 Ω) (P08) | erro; **igual desde 2026-11-14** (G3; instâncias com os mesmos valores compartilham a cópia) |
| S14 | avaliação preguiçosa | parâmetro indefinido num subcircuito não instanciado ou num `.model` não usado **não** é erro; erro num `.param` global **é** fatal (P05e, P05f) | avalia tudo; **igual desde 2026-11-14** (G3, também `.model` de topo) |
| S15 | binning | `nome` casa `nome.<dígitos>`; só tipos BSIM3/BSIM4/HiSIM; bin se `lmin ≤ L ≤ lmax` e `wmin ≤ W ≤ wmax` **inclusivo nas duas pontas** com tolerância absoluta 1e-9 (código BSD `in_range`); a tabela é percorrida do último `.model` definido ao primeiro (a fronteira L=1 µm caiu no bin definido por último); L e W multiplicados por `scale`; W dividido por `nf` se `wnflag` (padrão 1 em `hs`/`spe`); faltando um dos quatro limites o bin é pulado; nenhum bin → "could not find a valid modelname" (P09, P09b, P09c) | erro; **igual desde 2026-11-14** (G4, na expansão; `wnflag` padrão 0, modo normal) |
| S16 | `.option scale` | L/W dos bins e da instância BSIM4 × `scale`, AD/AS × `scale²`, PD/PS × `scale` (`b4par.c`); `lmin…wmax` **não** são escalados (P09b) | aviso "not implemented; ignored"; **igual desde 2026-11-14** (G5: também MOS 1–3/BSIM3 e `l w` de R/C; não MOS6/BSIM1/2, sonda `s6`) |
| S17 | `.if/.elseif/.else/.endif` | funciona no topo e em `.subckt`, condicionado a parâmetro da instância (P10) | erro |
| S18 | `m` na linha `X` | multiplica o subcircuito (2 × 1 kΩ → 500 Ω), **exceto** se a linha `.subckt` declara `m=` — aí `m` vira parâmetro comum e o multiplicador some (`m=4` → 1 kΩ; em `hs` o ngspice avisa "m=xx on .subckt line will override multiplier m hierarchy!") (P13) | erro; **igual desde 2026-11-14** (G6: o `m` do dispositivo é substituído; por tipo em `docs/dune-netlist.md`) |
| S19 | `X` que nomeia um **modelo** | erro com modelo nível 1 ("unknown subckt"), plain e `hs` (P14); **aceito** com o modelo binado do gf180 (`xmn … nmos_3p3`), plain e `hs`, mas Vout(0,9 V) muda de 3,2103 (linha `M`) para 3,2093 (plain) / 3,2052 V (`hs`) — causa não isolada | erro |
| S20 | `.include` indentado, `.inc`, aspas | aceitos (P12) | erro; **igual desde 2026-11-14** |
| S21 | `.params` | sinônimo de `.param` (P20) | erro; **igual desde 2026-11-14** |
| S22 | entrada hostil | ciclo de `.param` → erro "level depth greater 1000" (H1); **ciclo de `.include` → segfault** do ngspice (H2, rc=139); ciclo de `.lib` → erro espúrio "Missing second quote" (H3); 5000 parênteses → "Closing ) not found" (H4); cadeia de 20 000 `.param` → ok (H5); subcircuito recursivo → erro após ~22 níveis (H6) | H1: erro (por outra razão); H2: limite 16 ok; H6: limite 64, desde a G3 erro na primeira repetição; desde 2026-11-14 H2/H3 são erro "cycle" com span |

Versão do BSIM4 (`inpdomod.c`, BSD): `level=14|54` com `version` 4.0–4.5
→ `bsim4v5`, 4.6 → `bsim4v6`, 4.7 → `bsim4v7`, 4.8/"default" → `bsim4`
(4.8.3). Sensibilidade medida no inversor (deck achatado, só `version`
trocado):

| | 4.5 (como vem) × 4.8.3, máx. \|ΔV\| | 4.6 × 4.7 × 4.8.3 |
|---|---|---|
| sky130 DC (181 pontos) | 1,0e-10 V | idênticos |
| sky130 tran (10 ns) | 5,6e-4 V em t=1,065 ns (1,7409 × 1,7415 V) | idênticos |
| gf180 DC (331 pontos) | 9,2e-5 V em Vin=1,55 V | idênticos |
| gf180 tran | 1,6e-4 V em t=3,225 ns | idênticos |

Os 380 (sky130) e 265 (gf180) nomes de parâmetro dos modelos do inversor
existem todos nas tabelas `IOP` de `bsim4` (926 nomes) e de `bsim4v5`
(779); nenhum é exclusivo da 4.8.3.

## 6. Lacunas, em ordem

Tamanhos são **estimativas** em linhas de Rust com testes (referência: o
`dune-netlist` inteiro tem 6211 linhas; o porte do BSIM3 tem 5132 para
11 729 de C). "inv" = necessário para o inversor; "full" = para a
biblioteca inteira. **[API]** = muda API pública (precisa de aprovação do dono).

**G1 — Leitor: `.lib`/`.endl`, aspas, `.inc`, diretivas indentadas,
`.params`.** ✅ **Feito em 2026-11-14** (`deck.rs`; descrição em
`docs/dune-netlist.md`, seção "Bibliotecas `.lib`"; decks
`regress/lib_section.cir` e `regress/lib_nested.cir` com golden do
ngspice-47; diferenças no `KNOWN_DIFFS.md`). Limites escolhidos:
aninhamento 16 (`.include` + `.lib`), ciclo detectado por
(arquivo, seção), 256 MiB lidos, 256 MiB percorridos (cobrados a cada entrada, diretivas e comentários incluídos), 100 000 entradas e 1000 diagnósticos do leitor.
(a) inv: os três; full: os três. (b) S1, S20, S21. Seções selecionadas no
leitor, antes da junção de continuações; nome de seção case-insensitive;
caminho relativo ao arquivo que inclui (já é a regra do Dune). Ler cada
arquivo uma vez e fatiar seções (o gf180 pede `sm141064.ngspice` 8× no
inversor; o ngspice gasta 16 s no `.lib` do sky130 contra 1,7 s achatado).
(c) ~300 linhas; risco baixo. (d) Não, se `.lib` for resolvido dentro do
`deck` com o `IncludeLoader` atual; a mensagem de erro `.lib is not
supported` some. (e) `.lib` recursivo (H3) e `.include` cíclico (H2 —
segfault no ngspice) caem no limite de profundidade 16 já existente, que
deve contar `.lib` e `.include` juntos; o orçamento de 256 MiB deve contar
bytes **lidos**, não bytes emendados (reler a mesma seção não pode
multiplicar); a varredura por `.lib seção` é linear; aspas sem fechamento
viram diagnóstico; o loader do WASM continua decidindo o que é caminho
válido.

**G2 — Linguagem de expressão de parâmetros (semântica numparam, escrita
pelo manual).** ✅ **Feito em 2026-11-14** (`pexpr.rs` + `param.rs`;
descrição, tabela de precedência e as ~490 sondas que a fundamentam em
`docs/dune-netlist.md`, seção "Linguagem de expressões de parâmetros";
decks `regress/param_{ops,funcs,forms,tran}.cir` com golden do
ngspice-47; diferenças no `KNOWN_DIFFS.md`). Sem mudança de API
pública. Achados além do previsto abaixo: comparações são **um só
nível** associativo à esquerda; depois de um operador o `-` só forma
literal negativo (o resto o ngspice rejeita ou lê errado); `^` eleva o
módulo da base; não há constantes (`pi`, `e`); o sufixo `a` é atto; o
valor chega ao cartão arredondado por `%.15e` + `INPevaluate` (`{7}` →
7,000000000000001); aspas duplas somem das expressões de cartão (o
sky130 depende disso). Depois da G2, nos três inversores, **todos** os
erros de expressão sumiram; o que resta é G3 (`.param` em `.subckt`,
nomes de parâmetro de subcircuito, linha `X`), comportamental `v()`
(G11) e, no IHP, OSDI e `.if` — tabela em "Depois da G2", abaixo.
(a) inv: sky130 (`{…}` com espaços, `agauss`, `sqrt`, 11 851 `.param`
globais avaliados de saída — S14), gf180 (`'…'`, nomes nus em `.model`,
43 globais), IHP (`'…'`, `max`); full: os três. (b) S6–S11, S12. Gramática:
números com sufixo de escala (com `mil`=milli, S9), nomes, `+ - * /`,
`^`/`**` à esquerda, menos unário abaixo de `^`, comparações, `&& || !`,
`?:`, parênteses, chamadas; funções vistas nos PDKs: `agauss gauss aunif
unif sqrt abs min max pow pwr exp log ln log10 floor ceil int nint sgn
tanh cosh`; variável `temper`. Os delimitadores `{…}` e `'…'` precisam de
um modo léxico próprio: hoje `( ) , =` e branco são separadores de campo
no `Scanner` e cortam a expressão. É uma **segunda** linguagem, distinta da
árvore da fonte `B` (`expr.rs`, porte do `inpptree` do 3f5): lá `^` associa
à direita e `log` é base 10. (c) ~1200 linhas; risco médio (detalhes
numéricos: `pwr`, `nint`, NaN fatal). (d) **[API]** se `ir::ParamValue` ganhar uma
variante de expressão e se houver um `parse_param_expr` público; o
`parse_expr` público de hoje é o da fonte `B` e não deve mudar. (e)
recursão limitada (profundidade e nº de nós como `EXPR_MAX_*`); o limite de
512 bytes por expressão **não serve** (IHP tem 613) — sugerido 64 KiB;
NaN/∞ viram diagnóstico com span no uso (como o ngspice), não um valor
mudo.

*Depois da G2* (os mesmos `tb/<pdk>/inv.cir`, `dune` em `--release`;
`.claude/scratch/g2-probes/tbrun.py`, saídas em `tb-before/`,
`tb-after1/`):

| Classe | sky130 antes → depois | gf180 | IHP |
|---|---|---|---|
| `malformed parameter expression` | 3628 → 0 | — | — |
| `unexpected … in expression` | 177 → 0 | 72 → 0 | — |
| `` `X` cannot be a parameter name `` | 72 → 0 | 17 → 0 | — |
| `parameter has no value` / `expression ends…` | 8 → 0 | 1 → 0 | — |
| `unknown parameter` (todos dentro de `.subckt`: G3) | 475 → 4614 | 1 → 118 | — |
| `.param inside .subckt is not supported yet` (G3; antes vazava calado para o global) | 0 → 323 | 0 → 24 | — |
| `subcircuit parameters on X lines` (G3) | 119 → 119 | 8 → 8 | 2 → 2 |
| `v(…)` em valor (R comportamental, G11) | 0 → 24 | 0 → 4 | — |
| OSDI / `.if` em `.subckt` | — | — | 76 → 76 |
| **total** | 4479 → 5080 | 99 → 154 | 78 → 78 |

Os `unknown parameter` cresceram porque antes a expressão morria na
sintaxe e agora chega a procurar `l`, `w`, `mult`… — parâmetros da
instância que só a G3 define. Primeiro bloqueio de cada um: sky130 —
`.param` no corpo de `sky130_fd_pr__nfet_01v8` (G3); gf180 — `.param`
dentro dos subcircuitos de resistor/`_sab` do `sm141064.ngspice`
(linha 743), que o inversor nem usa (G3, parte "guardar sem avaliar");
IHP — `psp103va` (OSDI, fora de escopo).

**G3 — Escopo e avaliação adiada: parâmetros de subcircuito, linha `X`,
modelo por instância, preguiça.** ✅ **Feito em 2026-11-14** (`subckt.rs`
+ `param.rs`; regras, sondas (~150, `.claude/scratch/g3-probes/`) e
custos em `docs/dune-netlist.md`, seção "Parâmetros de subcircuito,
binning e `m`"; decks `regress/subckt_param_{scope,model,tran}.cir`;
diferenças no `KNOWN_DIFFS.md`). Sem mudança de API pública: o IR segue
pós-avaliação, o corpo fica em cartões e é avaliado por instância na
expansão. Achados além do previsto: as atuais da linha `X` são
avaliadas **no escopo do chamado** (`w={l}` lê o `l` dele); o escopo
é **dinâmico** (um subcircuito vê os parâmetros de quem o instancia);
uma definição que nomeia a si mesma lê o escopo de fora; atual sem
formal é ignorada (erro se o subcircuito não tem formal nenhuma).
Texto do plano original:
(a) inv: sky130 (padrões via `.param` no corpo, `X … w= l=`, 288 bins
especializados por instância — `toxe={…sqrt(l*w*mult)}`), IHP (padrões na
linha `.subckt`, `ng=`, `m=`); gf180 inv só precisa **guardar sem avaliar**
os 13 subcircuitos do alcance; full: os três. (b) S2–S5, S13, S14, S18.
Parâmetros globais como grafo de dependência (adiante permitido, último
vence, ciclo = erro); subcircuito expandido com o ambiente da instância;
`.model` local avaliado por instância; corpo de subcircuito e `.model`
guardados como texto/árvore e só avaliados quando instanciados/usados;
"dado" (given) preservado: `ad={ad}` com `ad=0` conta como dado no BSIM4
(o sky130 sempre passa `ad=0 as=0 pd=0 ps=0`). (c) ~2000 linhas; risco
**alto** — muda a expansão (`subckt.rs`) de estrutural para avaliada e o
custo de elaboração (288 cartões de ~300 parâmetros por instância de MOS
no sky130). (d) **[API]** `ir::Element`/`ir::Model`/`Netlist` passam a carregar
expressões ou o IR passa a ser pós-expansão por construção; os bindings
que expõem o netlist precisam acompanhar. (e) ciclos de parâmetro (H1),
profundidade de substituição, bomba de expansão (o orçamento de trabalho de
2²⁰ do `subckt.rs` precisa contar também as cópias de modelo por
instância), subcircuito recursivo (já limitado a 64).

**G4 — Binning.** ✅ **Feito em 2026-11-14**, na expansão (genérico por
nível: o BSIM4 herda), deck `regress/bin_bsim3.cir`; os coeficientes
`l*/w*/p*` do BSIM3 já eram aplicados pelo porte. Plano original:
(a) inv: sky130 e gf180; full: idem. (b) S15. (c) ~250 linhas no
`elaborate`; risco baixo-médio (tolerância absoluta 1e-9 m e ordem "último
definido primeiro" decidem as fronteiras; `binunit=2` é coisa do BSIM4).
(d) Não (interno), mas a mensagem "could not find a valid modelname" é
nova. (e) número de bins por base limitado pelo orçamento de modelos; L/W
não finitos ou ≤ 0 → diagnóstico.

**G5 — `.option scale` (e `wnflag`).** ✅ **Feito em 2026-11-14**
**sem campo novo em `Options`**: o `dune-netlist` dobra o `scale` na
geometria das instâncias durante a expansão (MOS 1–3, BSIM3/4: `l w` ×s,
`ad as` ×s², `pd ps` ×s; `R`/`C`: `l w` ×s; sonda `s6`) e o núcleo
para de avisar; `wnflag` só entra no binning. Muda o contrato do IR
(geometria pós-`scale`), não assinaturas — commit separado, para o
dono aprovar. Plano original:
(a) inv: sky130; full: sky130. (b) S16. (c) ~150 linhas (MOS de todos os
níveis, R, C, D nível 3, binning). (d) **[API]** campo novo em `Options`
(público). (e) `scale` ≤ 0 ou não finito → erro.

**G6 — `m` na linha `X` (multiplicador hierárquico).** ✅ **Feito em
2026-11-14** sem API nova (decks `regress/xmult_{rcl,semi}.cir`): `m`
nativo onde o núcleo tem (BSIM3/BSIM4/VDMOS, VBIC), valor/ganho/área
onde é equivalente, erro "not supported yet" com span em MOS 1–3/6,
BSIM1/2, `R`/`C` por geometria e fonte `I` transiente. No ngspice o
`m` do dispositivo é **substituído**, não multiplicado. Plano original:
(a) inv: nenhum dos testbenches mínimos; full: sky130 (107 linhas `X` com
`m=`), gf180 (subcircuitos com `m=1` na linha — o caso que **anula** o
multiplicador). (b) S18. (c) ~300 linhas; risco médio: todo dispositivo
precisa aceitar `m` (hoje só alguns). (d) **[API]** se as specs de dispositivo
ganharem `m`. (e) produto de `m` aninhados finito e > 0.

*Depois da G3/G4/G6* (os mesmos `tb/<pdk>/inv.cir`, `dune` em
`--release`; `.claude/scratch/g3-probes/tbrun.py`, saídas em
`tb-before/`, `tb-after1/`):

| Classe | sky130 antes → depois | gf180 | IHP |
|---|---|---|---|
| `unknown parameter` | 4614 → 0 | 118 → 0 | — |
| `.param inside .subckt is not supported yet` | 323 → 0 | 24 → 0 | — |
| `subcircuit parameters on X lines` | 119 → 0 | 8 → 0 | 2 → 0 |
| `v(…)` em valor (R comportamental, G11; estavam em subcircuitos não instanciados) | 24 → 0 | 4 → 0 | — |
| `unknown model type psp103va` (idem, agora preguiçosos) | — | — | 24 → 0 |
| `unknown device type` (`N`, OSDI) | — | — | 16 → 12 |
| `.if/.else/.endif` em `.subckt` (G9) | — | — | 36 → 36 |
| **total de erros do parser** | 5080 → 0 | 154 → 0 | 78 → 48 |

Primeiro bloqueio de cada um, agora na **elaboração**: sky130 —
`model xp.sky130_fd_pr__pfet_01v8__model.44: MOS level 54 is not
implemented yet` (bins 44/71 escolhidos, os mesmos do ngspice); gf180 —
`model pmos_3p3.8: MOS level 54` (bins `pmos_3p3.8`/`nmos_3p3.4`,
conferidos contra o ngspice pelo resultado); IHP — `.if` dentro de
`.subckt` (G9), depois OSDI. O inversor gf180 não instancia diodo; o
diodo nível 3 continua aceito com nota e simulado como nível 1 (G11)
onde for usado.

**G7 — `temper`.**
(a) inv: nenhum (aparece nos dispositivos de 20 V do sky130 e nos
resistores do gf180); full: os três. (b) S12. (c) ~150 linhas; risco
médio: um deck com varredura de temperatura (sweep/sessão) precisa
reexpandir, porque o valor entra na expansão. (d) Talvez (`Session`). (e)
nada novo.

**G8 — Funções aleatórias.**
(a) inv: sky130 (972, todas × `mc_mm_switch=0`); full: os três (gf180
`design.ngspice` liga `sw_stat_mismatch=1`, então os subcircuitos `_sab` e
resistores do gf180 são **aleatórios por execução** no ngspice e não servem
de oráculo sem desligar a chave). (b) S11. (c) ~100 linhas. (d) **[API]**
**decisão do dono**: o ngspice sorteia sem semente; a proposta é nominal
(média) por padrão, Monte Carlo com semente explícita opcional — desvio a
registrar em `KNOWN_DIFFS`. (e) nada novo.

**G9 — Condicionais `.if/.elseif/.else/.endif`.**
(a) inv: IHP; full: IHP. (b) S17. (c) ~300 linhas, por instância; risco
médio. (d) não. (e) aninhamento limitado; `.endif` faltando = erro com o
span do `.if`. Prioridade baixa: o IHP não é alvo de BSIM4.

**G10 — `X` chamando modelo binado.** (a) full gf180 (estilo do
`testing/smoke_test`); (b) S19 (e a diferença de resultado não explicada
precisa ser entendida antes); (c) ~100; (d) não. Opcional.

**G11 — Modelos além do BSIM4** (não é parser, mas decide o "full"):
diodo nível 3 com geometria e parâmetros de parede (os três PDKs; hoje o
Dune ignora `level` com nota e não tem `jsw`, `cjsw`…); resistor/capacitor
comportamental com `v()` (sky130 104, gf180 149 — Fase 8 item 6); modelos
`r`/`c` semicondutores completos; BSIM3v32 (sky130 células especiais);
`bsim4v6` (`version=4.62`/`4.6`); VBIC nível 9 (IHP, item 4); PSP/OSDI
(IHP, fora de escopo).

## 7. Desempenho do parser atual

`dune -b` sobre os arquivos reais (todos têm erros de parse, então o tempo
é de leitura+parse; wall e RSS máximo, uma rodada):

| Entrada | Bytes | Wall | RSS | Diagnósticos |
|---|---|---|---|---|
| `sky130_fd_pr__pfet_01v8_hvt__sf.pm3.spice` | 1 361 804 | 0,05 s | 25 MB | 792 |
| `sm141064.ngspice` (gf180) | 1 348 553 | 0,03 s | 26 MB | 521 |
| `parameters/montecarlo.spice` (cartão de 1,27 MB) | 1 279 155 | 0,12 s | 13 MB | 12 795 |
| inversor sky130 achatado | 7 136 950 | 0,16 s | 88 MB | 4362 |
| o achatado repetido 8× | 57 095 396 | 0,98 s | 360 MB | 7218 |

Varredura de pânico/travamento: os 821 + 3 + 38 arquivos e os decks
achatados (869 entradas), cada um com alarme de 20 s: 283 terminaram com
código 0 e 586 com 1 (diagnóstico); **nenhum pânico, sinal ou timeout**;
14 s no total. Ponto de atenção: ~6× de RSS sobre o texto (360 MB para
57 MB); no teto de 256 MiB do leitor isso daria ~1,6 GB (**estimativa**) —
a guardar quando G3 criar cópias por instância.

## 8. Defeitos do Dune atual achados de passagem

Os itens 1, 2, 3 e 5 foram corrigidos em 2026-11-14 junto com a G1; o 4, com a G5.

1. ✅ **`^` à direita em `{…}`**: `{2^3^2}` = 512 no Dune, 64 no ngspice-47
   (`param.rs` comenta "Right-associative, like ngspice", e o
   `docs/dune-netlist.md` repete). Reprodutor: `D_POW` no Apêndice A.
   *Corrigido*: à esquerda, expoente com sinal (`2^-1`); deck
   `regress/param_pow.cir`. A fonte `B` (outra gramática, porte do
   `inpptree.c`) também difere do ngspice-47 (64 e −4 lá, 512 e 4 aqui)
   e fica no 3f5 (`KNOWN_DIFFS.md`).
2. ✅ **`.include "arq"`**: as aspas entram no caminho
   (`cannot open .include file …/"…"`); o ngspice aceita aspas simples e
   duplas. *Corrigido* com o leitor de diretivas da G1.
3. ✅ **`X … nome=valor`**: o último token vira nome de subcircuito ("no
   such subcircuit `0.15`"); diagnóstico enganoso enquanto G3 não existe.
   *Corrigido*: erro "subcircuit parameters on X lines (`w=…`) are not
   supported yet" com o span do nome do parâmetro (também `params:`).
4. ✅ **`.option scale` só avisa** e segue com geometria errada.
   *Corrigido* com a G5 (2026-11-14).
5. ✅ **Valor não finito em `.param`**: `.param z={1/0}` passa calado; o deck
   P05d termina em "resistor cannot have zero resistance" para um NaN.
   *Corrigido* como no ngspice-47 (sondas de 2026-11-14): definir ou
   repassar um não finito entre `.param` não é erro (`{1/z}` volta a 0
   e roda), o `{…}` não finito que chega a elemento, `.model` ou cartão
   de controle é erro com span.
6. **Diodo `level=3`**: aceito com notas, simulado como nível 1 (sem
   geometria nem parede) — resultado errado silencioso até G11.

Nenhum travamento nem pânico com entrada real.

## 9. Primeiro alvo e BSIM4 mínimo

**gf180mcu (models/ngspice do repositório Google) é o primeiro alvo.**
Para o inversor: G1 + G2 (sem funções no fecho; aspas, nomes nus em
`.model`, `==`/`?:` só para guardar subcircuitos não usados) + a parte
"guardar sem avaliar" de G3 + G4. Sem `scale`, sem subcircuito invólucro,
sem modelo por instância, sem `.if`. Depois sky130 (G3 inteiro + G5 + G8).

**BSIM4 mínimo para o inversor gf180** (`nmos_3p3`/`pmos_3p3`, 16 bins
cada, 265 parâmetros distintos):

- versão: cartões `4.5` encaminhados ao núcleo 4.8.3 (D-0060), com aviso;
  oráculo bit a bit contra o ngspice com `version=4.8.3` reescrito,
  tolerância contra o PDK original (medido: ≤ 92 µV DC, ≤ 0,16 mV tran);
- modos usados (todos os 32 cartões): `mobmod=0 capmod=2 igcmod=0 igbmod=0
  geomod=0 rgeomod=0 diomod=1 rdsmod=0 rbodymod=0 rgatemod=0 permod=1
  acnqsmod=0 trnqsmod=0 fnoimod=1 tnoimod=0 tempmod=0 binunit=2
  paramchk=1`; `mtrlmod`, `cvchargemod`, `wpemod`, `gidlmod` no padrão;
- binning: G4 + coeficientes `l*/w*/p*` (113 nomes) com `binunit=2`;
- instância: `w`, `l` (a biblioteca completa usa ainda `as ad ps pd nrd
  nrs nf sa sb sd delvto dtemp m`);
- stress LOD: parâmetros presentes em todos os cartões mas **inertes** sem
  `sa>0` e `sb>0` (`b4temp.c:1657`); WPE ausente; GIDL presente;
  tunelamento de porta com `igcmod=igbmod=0` (inerte).

sky130 pede o mesmo núcleo com `rbodymod=1` (rede de resistência de
corpo), `tnoimod=1`, `tempmod=0`, instância `l w nf ad as pd ps nrd nrs sa
sb sd` (dados como 0), 180+108 bins e `scale=1u`.

**Não-BSIM4 dos dispositivos básicos.** gf180: diodo nível 3, resistores
de difusão/poli como subcircuito com R comportamental `v(1,2)` e
`temper`, MIM `c`, BJT nível 1. sky130: diodo nível 3, `r`/`c` com
modelo, R comportamental (20 V), BSIM3v32 (células especiais), BJT nível
1. IHP: PSP 103.6 (OSDI), VBIC nível 9, diodo níveis 1/3, `r3_cmc`,
`cap_cmom`, `mosvar` (OSDI).

## 10. O que não foi medido

- **IHP no ngspice**: sem OpenVAF nem `.osdi` (não estão no repositório;
  baixar binário de terceiros para executar ficou fora das regras desta
  tarefa). O inversor IHP não tem referência.
- **gf180 do open_pdks** (`gf180mcuD`, nomes `nfet_03v3`…): não há release
  do gf180 entre as 100 últimas do `ciel-releases`; medido só o repositório
  Google, cujos nomes de dispositivo (`nmos_3p3`) diferem do que xschem usa.
- A causa da diferença `X`×`M` em S19 e da diferença `hs`×plain (~1 mV,
  da ordem de `reltol·|V|`) não foi isolada.
- A semântica do `wnflag` com `nf>1` e dos sufixos de escala em todos os
  contextos (`meg` × `m` em `.model`) não foram sondadas.
- Desempenho do BSIM4 e da expansão por instância: inexistentes.

## Apêndice A — sondas no ngspice-47

Rodadas em `.claude/scratch/pdk/work/tb/probes/` (`bin/probes*.sh`),
`ngspice -b`, sem `.spiceinit` salvo onde dito. Valores lidos com
`.control` / `op` / `print`. Resistor de carga entre o nó e a terra quando
o valor é de fonte; correntes impressas como `1/i(v)` (o sinal negativo é
a convenção do ngspice).

**P01 `.lib`** — `sub/libA.lib`:

```
.LIB SEC1
.param a1=1
.lib 'libA.lib' sec2
.include '../inc1.inc'
.ENDL SEC1
.lib sec2
.param a2=20
.endl
.lib unused
.param a3=this is not parsed (
.endl unused
```

`inc1.inc`: `.param i1=300`. Deck: `.lib "sub/libA.lib" sec1`,
`v1 1 0 {a1+a2+i1}` → `v(1) = 3.210000e+02`.

**P02** `.param a={b+1}` / `.param b=2` / `v1 1 0 {a}` → `3.000000e+00`.
**P03** `.param c=1` / `v1 1 0 {c}` / `.param c=5` → `5.000000e+00`.

**P04 precedência**

```
.param w=7
.subckt sbody a
.param w=1 l=2
r1 a 0 {w*1000+l}
.ends
.subckt sline a w=1 l=2
r1 a 0 {w*1000+l}
.ends
.subckt skw a params: w=1 l=2
r1 a 0 {w*1000+l}
.ends
.subckt sglob a
r1 a 0 {w*1000}
.ends
x1 n1 sbody w=5
x2 n2 sline w=5
x3 n3 skw w=5
x4 n4 sglob
x5 n5 sbody
```

→ `-5.00200e+03 -5.00200e+03 -5.00200e+03 -7.00000e+03 -1.00200e+03`.

**P05 expressões** (cada `.param eN` alimenta `vN N 0 {eN}`; `e8`
`pow(-2,0.5)` e `e16` `sqrt(-4)` retirados porque abortam a rodada com
"unknown parameter (nan)"):

| | expressão | valor |
|---|---|---|
| e1 | `{ 2 + 3 * 4 }` | 14 |
| e2 | `'2**3**2'` | 64 |
| e3 | `{2^3^2}` | 64 |
| e4 / e5 | `{-2^2}` / `{-2**2}` | -4 / -4 |
| e6 / e7 | `{pwr(-2,3)}` / `{pow(-2,3)}` | 8 / -8 |
| e9 | `{(1>0) ? 10 : 20}` | 10 |
| e10 | `{(1==1) + (2!=2) + (1<=2) + (3>=4)}` | 2 |
| e11 | `{1 && 0 \|\| 1}` | 1 |
| e12 | `{min(3,4) + max(3,4)}` | 7 |
| e13 | `{floor(2.7) + ceil(2.1)}` | 5 |
| e14 | `{log(100) + ln(1)}` | 4,605170 |
| e15 | `{2k*3}` | 6000 |
| e17 | `{abs(-3) + int(2.9) + nint(2.5)}` | 7 |
| e18 | `{7/2}` | 3,5 |
| e19 | `{limit(5,0,3)}` | 8 |
| e20 | `{!0}` | 1 |
| e21 | `{1meg/1e6 + 1mil/25.4e-6}` | 40,37008 |
| e22 / e23 / e24 / e25 | `{exp(1)}` / `{log10(1000)}` / `{sgn(-3)}` / `{2.5e-3/1u}` | 2,718282 / 3 / -1 / 2500 |

**P05b** `.PARAM Q1 = ' 1 + 2 '`, `.param q2 = q1*10`,
`.param q3={Q1 * 100}`, fontes `'q1'`, `q2`, `{q3}` → 3, 30, 300.
**P05c** `.model rm r rsh={sqrt(-4)}` não usado → roda normal.
**P05d** `.param z={1/0}`, `r2 2 0 {1k+z*0}` → "unknown parameter
(nan)", nenhuma simulação.
**P05e** `.param z={foo(1)}` → "Undefined parameter [foo]", fatal.
**P05f** `{nosuch*2}` dentro de subcircuito não instanciado e
`.model rmu r rsh={nosuch2}` não usado → roda normal.

**P06** `g1=agauss(1,0.1,1)`, `g2=gauss(1,0.1,1)`, `g3={0*agauss(0,1,1)}`,
`g4=aunif(1,0.1)`, `g5=unif(1,0.1)`: 1ª rodada 0,9708 0,9786 0 1,0609
0,9374; 2ª rodada 1,0276 1,0330 0 0,9390 0,9243.

**P07** `.options temp=50`, `.param t1={temper}` → "Undefined parameter
[temper]", fatal. **P07a** `.model rm r rsh={temper}`, `r2 2 0 rm l=1 w=1`
→ 50 Ω. **P07b** `r2 2 0 r={temper}` → 50 Ω. **P07c** `.param
rt='temper*2'` dentro de `.subckt` → 100 Ω. **P07d** `.temp 75`,
`r={temper}` → 75 Ω.

**P08**

```
.subckt rs a w=1
.model rmod r rsh={w*100}
r1 a 0 rmod l=1 w=1
.ends
x1 n1 rs w=1
x2 n2 rs w=3
```

→ 100 Ω e 300 Ω.

**P09 binning**

```
.model mm.1 nmos level=49 version=3.3 lmin=0.1u lmax=1u wmin=1u wmax=10u vth0=0.3
.model mm.2 nmos level=49 version=3.3 lmin=1u lmax=10u wmin=1u wmax=10u vth0=0.6
m1 d1 g 0 0 mm l=0.5u w=2u
m2 d2 g 0 0 mm l=1u w=2u
m3 d3 g 0 0 mm l=5u w=2u
m4 d4 g 0 0 mm l=1.0005u w=2u
```

`show m : model` → m1 `mm.1`; m2 (fronteira), m3, m4 `mm.2`.
**P09b** os mesmos modelos com `.option scale=1u` e `l=0.5 w=2` / `l=5
w=2` → `mm.1` (l=5e-07) e `mm.2` (l=5e-06). **P09c** só `mm.1`, `l=50u` →
"could not find a valid modelname".

**P10**

```
.param top=1
.subckt sif a sel=0
.if (sel == 0)
r1 a 0 1k
.elseif (sel == 1)
r1 a 0 2k
.else
r1 a 0 3k
.endif
.ends
x0 n0 sif sel=0
x1 n1 sif sel=1
x2 n2 sif sel=2
.if (top > 0)
rt nt 0 5k
.else
rt nt 0 6k
.endif
```

→ 1k, 2k, 3k, 5k.

**P11** `.param rv=123 wv=2`, `.model rm r rsh=rv`, `r1 1 0 rm l=1 w=wv` →
61,5 Ω. **P12** `  .include 'inc1.inc'` (indentado) e `.inc "inc1.inc"` →
`v(1)=300`.

**P13**

```
.subckt r1k a
r1 a 0 1k
.ends
.subckt r1km a m=1
r1 a 0 1k
.ends
x1 n1 r1k m=2
x2 n2 r1km m=4
```

→ 500 Ω e 1000 Ω.

**P14** `.model nm nmos level=1 …`, `xm1 d g 0 0 nm w=10u l=1u` → "unknown
subckt", plain e com `set ngbehavior=hs`. Com o gf180 (`xmp`/`xmn` no
lugar de `mp`/`mn` no `inv_dc.cir`) roda: Vout(0,9 V) = 3,209300 (plain),
3,205244 (`hs`), contra 3,210277 com `M`.

**P15** `.param g=10`; `.subckt sc a w=1` com `.param lw={w*g}` e
`.param lw2={lw+1}`, `r1 a 0 {lw2*100}`; `x1 n1 sc w=2` → 2100 Ω.
**P17** `.subckt rr a b lr=1` / `r1 a b {lr*1k}`, `.param d_sab=0`,
`x1 1 0 rr lr='(d_sab==0) ? 2 : d_sab'` → 2000 Ω.
**P18** `.param l=9`; `s1` com `l=1` na linha, `s2` sem; `x1 1 s1`,
`x2 2 s2`, `x3 3 s1 l=3` → 1k, 9k, 3k.
**P19** `.param AbC=4`, `{abc*ABC}` → 16. **P20** `.params` / `+ pp=7` → 7.
**P21** `.lib s1` … `.endl wrongname` → aceito. **P22** seção inexistente
→ "ERROR, library file sub/libB.lib, section definition nosuch not
found", fatal. **P23** `.subckt s a` / `.param ad=0 w=1` /
`r1 a 0 {1000+ad+w}`, `x1 1 s ad=5` → 1006 Ω. **P24** a mesma seção duas
vezes → normal.

**H1** `.param a={b+1}` / `.param b={a+1}` → "A level depth greater 1000
for dependent parameters is not supported!", fatal.
**H2** `h2_inc.inc` contendo `.include 'h2_inc.inc'` → *Segmentation
fault* (rc 139).
**H3** `.lib s` / `.lib 'h3_lib.lib' s` / `.endl` → "Missing second quote
in line .lib 'h3_lib.lib", fatal.
**H4** `{` + 5000 `(` + `1` + 5000 `)` + `}` → "Closing ")" not found",
fatal. **H5** 20 000 `.param pN={pN-1+1}` → `v(1)=20000`, ok.
**H6** `.subckt sa a` / `xa a sa` / `.ends` → "unknown subckt:
x1.xa.xa…(22 níveis)", fatal.

**D_POW** (Dune × ngspice):

```
.param p={2^3^2}
v1 1 0 {p}
r1 1 0 1
v2 2 0 {2^3^2}
r2 2 0 1
.op
```

ngspice-47: `V(1)=V(2)=6.400000e+01`; Dune: `v(1)=v(2)=5.120000e+02`.

**Versão do BSIM4**: `work/bin/s6.sh` achata `inv_dc`/`inv_tran` e
reescreve `+ version = 4.5` (916 cartões no sky130, 34 no gf180) para
4.6/4.7/4.8.3; diferenças por `work/bin/vdiff.py` (mesmo índice de
ponto, Δx = 0).

## Apêndice B — reprodução

```sh
W=/Users/leite/Github/dune/.claude/scratch/pdk/work
bash $W/bin/mktb.sh                # gera tb/<pdk>/inv*.cir (caminhos absolutos)
bash $W/bin/s2.sh                  # ngspice nos decks divididos
bash $W/bin/s3.sh; bash $W/bin/s4.sh   # Dune: deck original; deck achatado
bash $W/bin/s5.sh                  # censo -> census/*.json
python3 -I $W/bin/closure.py $W/tb/sky130/flat.cir
bash $W/bin/s6.sh                  # versões do BSIM4
bash $W/bin/s7.sh; bash $W/bin/s8.sh   # desempenho; varredura de pânico
bash $W/bin/probes.sh              # sondas (e probes2..4.sh)
```

Os scripts usam `S=$W` e os caminhos absolutos do PDK em
`.claude/scratch/pdk/`; o binário do Dune vem de
`~/.cargo/agent-target-four/release/dune` (`rundune.sh`, `s7.sh`,
`s8.sh`). `probes.sh` apaga e recria `tb/probes/`.
