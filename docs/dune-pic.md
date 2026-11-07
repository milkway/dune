# dune-pic — extrator de netlist de figuras rpic

Fase 5 (PLAN §2.5; D-0003/D-0004/D-0019/D-0020/D-0021). Depende de
`rpic-core = "=0.16.1"` (crates.io, zero deps, wasm-friendly) — só
API pública, como o D-0003 manda. Os três interinos do D-0020
foram aposentados na 0.13.0 (PRs #414/#415/#416), e os shims de
compat na 0.16.0 (rpic-lang #425 E1/E2): continuações `+` vêm
dobradas no `Drawing.passthrough` e argumentos crus em
`MacroCall.raw_args`.

## Pipeline

1. **Diretivas**: vêm de `Drawing.passthrough` (linhas `.` de
   **coluna 1**, regra do dpic real — indentado é erro de compile,
   testado). As continuações `+` de coluna 1 já vêm dentro da
   entrada do passthrough (cada linha aparada; `end_line` fecha o
   span).
2. Compila prelude (`corpus/pic/spice.pic`, embutido) + figura com o
   rpic (std `circuits: true`; `copy` negado). Exige `spice = 1` nos
   `vars`.
3. Colhe blocos rotulados (label = refdes; 1ª letra = cartão, só
   R C L V I D Q M J Z S W X T O E F G H K — o K não tem pinos, só o
   marcador `Mutual`), marcadores de pino invisíveis, blocos
   `ground()` (nó 0), **places nomeados** (`Vout: (2.5,1.5)` —
   D-0004 §3 como desenhado; `Drawing.places`, só os da fonte do
   usuário), fios (todo vértice de polilinha é terminal) e `dot`s.
   Um place vira terminal (snap/junção em T valem); place no vazio
   → warning; label de place colidindo com refdes →
   NodeNameConflict; place no terra → warning (places são nomes).
4. **Conectividade**: union-find com `snap` (default 1e-3"), junção
   em T (terminal no interior de segmento conecta — regra nossa;
   cruzamento puro NÃO conecta, `dot` resolve), `node()` iguais
   unificam como net labels.
5. **Nomes**: `0` do ground, depois os places, depois `n1, n2…`
   por menor índice de shape (determinístico).
6. **Emissão**: netlist SPICE3 textual. **Valor/modelo** = o
   argumento do call-site lido CRU de `MacroCall.raw_args` (por
   argumento; chamada quebrada em linhas também — `2.5e3` fica
   `2.5e3`), com fallback no normalizado só para argumento ele
   mesmo quebrado por `\`, e para o
   texto desenhado quando não há call de uma linha só ou um macro
   embrulhador intercepta (guardado por comparação pós-normalização,
   testado). O prelude segue desenhando o valor (apresentação).
   Diretivas na ordem da fonte
   (+ continuações `+`; `.end` do usuário é deduplicado) — passa
   pelo `dune-netlist` normal. `OverlayMap` dá refdes→`sN..sM` +
   `pins` (nome, rede, ponto do terminal em polegadas pic, no
   referencial da scene) e net→shapes — nets ligadas pino a pino
   têm `shapes` vazio e os pins são o alvo de probe (issue #4).
7. **Diagnósticos** com o span do call-site do usuário
   (`BlockInfo.span` — duplicata aponta a linha do usuário,
   testado): pino solto, net flutuante,
   valor faltando (warning em V/I — SPICE trata como 0 — e erro no
   resto), refdes duplicado, letra desconhecida, sem `spice = 1`,
   conflito de nome de nó. Referências são checadas depois da
   coleta: F/H sem um V de controle existente e K sem dois L
   existentes são **erro** (MissingValue, com span; o cartão ainda
   sai); substrato de Q com o nome do modelo é warning (regra do
   INP2Q, ver ressalvas).

## Convenções pic (D-0020)

- Pinos: 2-terminais `P`/`N` (P = 1º ponto e 1º nó: + de fonte,
  anodo, cauda da seta); Q `C/B/E`; J `D/G/S`; M `D/G/S/B` (B
  desenhado amarrado ao S); Z `D/G/S` (como J); X `Inp/Inm/Out` (opamp)
  ou `P1…PN` (subcircuito genérico, ordem do `.subckt`); S `P/N/Cp/Cn`; W
  `P/N` com valor `Vctl modelo`; T/O `P1/N1/P2/N2` (porta 1 +/−,
  porta 2 +/−, a ordem do cartão `T1 p1 n1 p2 n2 Z0=50 TD=1n`);
  E/G `P/N/Cp/Cn` (4 pontos, ganho no 5º argumento — o par de
  controle é desenhado nos pontos do usuário, terminal aberto +
  stub, como a chave); F/H `P/N` com valor `Vname ganho`; Q ganha o
  substrato opcional `S` (`npn4/pnp4`, C/B/E/S, stub no lado do
  coletor); `nmos4/pmos4` têm o bulk `B` como stub próprio
  (D-0026).
- `coupling(L1, L2, k)` (K) não tem pinos: desenha um arco `<->`
  entre os meios das duas bobinas (rótulo `k=…`; empilhadas, o
  rótulo justifica para fora) e emite `K1 L1 L2 k`. Os dois L
  precisam estar **desenhados antes** — rótulo inexistente é erro
  de compilação do pic com span na linha do usuário.
- `tline(P1, P2, params)` / `ltraline(P1, P2, model)` são macros de
  **2 pontos**: o estádio coaxial de P1 a P2 é o condutor de sinal;
  os retornos N1/N2 saem da blindagem perto de cada extremidade e
  terminam 0.4" à direita do sentido P1→P2 (abaixo, numa linha
  esquerda→direita) — o usuário liga fios a `T1.N1`/`T1.N2`.
- Labels pic exigem maiúscula (por isso não há `R1.a`);
  `Gnd`/`At` são reservados pelo prelude.
- Macros do prelude: resistor, capacitor, inductor, diode, vsource,
  acsource, battery, isource, vcvs, vccs, cccs, ccvs, coupling,
  switch, cswitch, npn, pnp, npn4, pnp4, nmos, pmos, nmos4, pmos4,
  njfet, pjfet, nmes, pmes, opamp, subckt2…subckt7, tline, ltraline,
  ground. (O `node()`
  interino saiu — nomes de nó são places.)
- Valor desenhado na figura (acima do elemento horizontal, à
  esquerda do vertical) — é o que o extrator lê.

## Subcircuito genérico (X) e MESFET (Z) — 5ª onda

- **`subckt2(P1, P2, nome)` … `subckt7(P1, …, P7, nome)`**: instância
  `X` de um `.subckt` com N pinos nos N pontos do usuário, na ordem do
  cartão (`X1 P1 … PN nome`); pinos `P1…PN`. A aridade vai no nome da
  macro — é o mecanismo do pic de Kernighan (D-0004: argumentos de
  macro, sem dialeto): o nome do subcircuito continua sendo "o
  argumento depois dos pontos", como em todo elemento, e 7 pinos +
  nome + display enchem exatamente os `$1..$9` do pic clássico. A
  alternativa variádica (`$+` do dpic, que o rpic tem) obrigaria o
  nome a vir **antes** dos pontos — quebraria a regra do valor e
  tornaria o display opcional ambíguo. Display (D-0026) vale.
- Desenho: o retângulo gerado pelos pontos (mínimo 0,6 × 0,4", crescido
  em torno do centro) — pontos no contorno são pinos na borda, como
  numa folha hierárquica; ponha-os lá. Nome (ou display) no centro; o
  número de cada pino em `fontsize 7`, por dentro, recuado da(s)
  borda(s) em que está (canto: das duas).
- Extrator: `X` com marcadores `P1…PN` consecutivos é o genérico
  (valor = argumento N); sem eles, segue o opamp (`Inp/Inm/Out`,
  valor = 2º). As duas formas convivem na mesma figura (testado);
  as 27 figuras anteriores dão SVG, netlist, OverlayMap e
  diagnósticos idênticos byte a byte antes/depois da mudança no
  prelude. `principal` = `None`; o span de write-back é o nome.
- **`nmes(At, modelo)` / `pmes(At, modelo)`** → cartão `Z D G S
  modelo [area]`, pinos `D/G/S` como o JFET. Glifo: o corpo do JFET do
  std (`__njfet/__pjfet`, barra de canal e seta de polaridade no
  gate) com a **marca Schottky da IEEE 315** na barra de canal — os
  dois ganchos que distinguem o diodo Schottky do de junção, aqui
  distinguindo gate metal-semicondutor de gate p-n. A seta fica: é a
  polaridade NMF/PMF que o modelo exige. Os ganchos saem para fora
  (simetria de ponto, como no diodo): para dentro encostariam nos
  terminais de dreno e fonte. Não há símbolo de MESFET normatizado
  (IEEE 315 não tem; livros de GaAs usam o do JFET), então a escolha é
  a menor marca legível que separa Z de J. Write-back: `principal` =
  `area`, parâmetro `area` (posicional ou `AREA=`; o MES não tem
  `temp`).

## Write-back de valores e frame do SVG (issues #17, #19)

Para editores (o Studio) que escrevem o valor de um slider de volta na
fonte pic, cada `ElementOverlay` traz:

- `value: Option<ValueSpanOverlay>` — `{span: PicRange, text}` do
  argumento de valor/modelo **na fonte do usuário**. `PicRange` =
  `line, col, end_line, end_col`, 1-based como os diagnósticos, fim
  exclusivo, **coluna em chars** (code points; o JS indexa com
  `Array.from(linha)`, não por unidade UTF-16). Argumento quebrado por
  `\` dá span multi-linha com o texto verbatim; argumento entre aspas
  (`"DX"`) dá o miolo. Só sai `Some` quando a chamada mais externa é
  um macro de elemento do próprio prelude (os `define` do prelude que
  não começam com `__`) e o argumento cru está na fonte do usuário —
  wrapper do usuário (mesmo um que repassa o valor), bloco inline,
  `K` (três argumentos) e fallback do desenho dão `None`: não há span
  confiável. O display (argumento depois do valor) não mexe no span.
  Defesa extra: a caminhada de chars tem que cair exatamente no
  `end_line/end_col` que o rpic reporta, senão `None`.
- `params: Vec<ParamSpanOverlay>` — `{name, span, text}` dos
  parâmetros alteráveis grafados dentro do valor, com os nomes do
  `alter` (`dune_core::Circuit::alter`): V/I `dc`, `acmag`, `acphase`
  (só o número: `AC 1 90` → `1` e `90`) e `pulse`/`sin`/`exp`/`pwl`/
  `sffm`/`trnoise`/`trrandom` (a grafia inteira, palavra e parênteses);
  R/C/L/E/G o principal (número inicial) e `ic` (`IC=`, C/L); F/H
  `gain`; D/Q/J `area` (número após o modelo ou `AREA=`) e `temp`; Z `area`;
  M `w`, `l`, `temp`. Só o que está grafado; repetido → a última
  grafia. Parse sobre o texto cru (`spans.rs`), offsets em chars →
  posições absolutas pelo início do argumento.
- `principal: Option<String>` — o que `alter(refdes, value)` muda, por
  letra (`resistance`, `capacitance`, `inductance`, `dc`, `gain`,
  `area`, também Z; `None` em M S W X T O K).

`OverlayMap.canvas: CanvasOverlay {w, h, origin, rescale}` é o frame
do SVG do `render_svg`/`renderPic`, com a fórmula exata

```text
sx = (x + origin[0]) · 96        sy = (h − (y + origin[1])) · 96
```

(`CanvasOverlay::to_svg`). Derivada do `svg.rs` do rpic-core 0.16.1:
`Svg::p` mapeia `(x − west + 2·pad + m.left, north − y + pad + m.top)
· 96`, e a scene publica `origin = (2·pad + m.left − west, h − north −
pad − m.top)`. O `rescale` **não entra** (as coordenadas, pins
incluídos, já são pós-rescale) e o sinal é `+origin` — a fórmula
chutada na issue estava errada nos dois pontos. A scene arredonda a 6
casas; aqui a translação vem de `svg::place_points` com um place-sonda
na origem (a mesma função com que o SVG desenha) e `w/h` repetem o
`canvas_frame` (união dos bboxes de `object_geometries`, ou o
`canvas` fixo, + padding + margens). Testes: todo pin cai num endpoint
de stroke do SVG a 1e-6 (rc, margens + `linethick`, `.PS 1.3`,
`canvas from … to …`), viewBox = `w·96 × h·96` nas 14 figuras do
corpus, e o header da scene bate a 5e-7.

## Display (rótulo curto no desenho)

- Argumento opcional **depois** do valor/modelo: quando não vazio, o
  prelude desenha o display no lugar do valor —
  `vsource(In, (0,0), PULSE(0 5 0 1u 1u 10m 20m), "G₁")` desenha só
  "G₁"; o cartão continua com o PULSE completo. Aceitam display: os
  2-terminais com valor (R C L D, v/ac/i-source, battery, cccs,
  ccvs), vcvs/vccs (6º argumento) e os centrados em `At`
  (npn/pnp/npn4/pnp4, nmos/pmos/nmos4/pmos4, njfet/pjfet, nmes/pmes,
  opamp) e as caixas `subckt2…7` (argumento depois do nome).
  coupling, chaves e linhas ficam de fora.
- Todo bloco que desenha display carrega o marcador reservado
  `Disp`. O extrator só reconhece display quando a chamada mais
  externa é a **própria macro do prelude** com exatamente um
  argumento a mais; aí o guard compara o desenho com o display e o
  valor segue vindo cru do argumento posicional.
- Armadilhas fechadas (testadas): wrapper com um argumento a mais
  cai no fluxo antigo (guard contra o valor → fallback no desenho);
  wrapper que **repassa** um display não tem valor confiável — o
  `Disp` sem chamada verificável vira erro `MissingValue` com span
  ("draws a display text… but its call is not the element's own
  macro").
- Subscritos: use Unicode ("T₁", "G₁") — o `$T_1$` do texlabels não
  renderiza no caminho rpic-core/wasm do dune (issue #7 cobre
  registrar o renderer de math no caminho nativo).
- Convenção do corpus: **G₁** geradores/sinais, **T₁…** transistores,
  **VCC/VEE/VDD** alimentações.
- Pega: nome curto de macro do usuário pode colidir com variável de
  rascunho do std (ex.: `tr`, usada pelo `__bjt` — "macro expansion
  too deep"); prefixe os seus.

## Compat

Nenhum shim desde o rpic-core 0.16.1. O prelude desenha os símbolos
que sombreia pelos gêmeos de geometria do std 0.16
(`__switch/__nmos/__pmos/__njfet/__pjfet`, como já fazia com
`__resistor`, `__bjt`, `__opamp` & cia.) em vez de cópias próprias
(dune#18; as 14 figuras do corpus deram SVG, netlist e OverlayMap
idênticos byte a byte antes/depois). Só o `battery` segue copiado
(`__dn_battery`, BSD-2): o std não tem `__battery`. Labels em macro
flat (E3) não são usadas — os elementos continuam blocos rotulados.

## Ressalvas conhecidas

- Regra de junção em T é interpretação nossa (ferramentas de
  esquemático fazem assim; endpoint-only tornaria `dot` no meio de
  fio inútil).
- `battery` é a única geometria do std copiada no prelude
  (`__dn_battery`, sem gêmeo `__battery` no 0.16): mudança no
  `battery` do std não chega ao Dune até a cópia ser refeita.
- `pmos`: o std chama `gDrain` o terminal de cima; o prelude mapeia
  S e B em cima e D embaixo (convenção de símbolo PMOS).
- `vsource` chama `voltage_source($2,$1)` invertido para o `+`
  encarar o P.
- `PRELUDE` usa `include_str!` com caminho fora do crate
  (`corpus/pic/spice.pic`) — `cargo package` do dune-pic falharia;
  irrelevante enquanto não publicamos o crate.
- `X` (opamp ou `subcktN`) emite o cartão, mas o corpo `.subckt` não cabe em linhas `.`
  (linhas de corpo não começam com `.`) — precisa de `.include`,
  resolvido relativo à figura. Testado em `opamp.pic` +
  `opamp_ideal.sub` (ganho fechado com A finito a 1e-5).
- O `define tline` do prelude **sombreia** o `tline` do std
  `circuits` do rpic (assinatura diferente) — deliberado: dentro de
  uma figura `spice = 1` a versão SPICE é a que vale.
- Place redefinido num laço funde as nets pelo nome (semântica de
  net label); um "cursor" rotulado móvel curto-circuitaria em
  silêncio.
- Cartão Q de 4 nós: o INP2Q do 3f5 só trata o 4º token como
  substrato se ele **não** for nome de `.model` — uma net com o
  nome do modelo seria lida como modelo. O extrator avisa
  (NodeNameConflict, warning).
- No losango horizontal do vcvs/ccvs o "−" sai como barra
  perpendicular ("+ |") — comportamento do `vsource_ctrl`/
  `voltage_source` do std, mantido.
- O `snap` se aplica depois do auto-scale do rpic (`maxpswid`);
  prelude põe `margin = 0.1`.

## Corpus e validação

`corpus/pic/{rc,rlc,rect,bjtamp,opamp,tline,bandpass,cmosinv}.pic`
+ goldens revisados à mão em `corpus/pic/golden/*.cir`; testes
comparam o texto extraído, provam que parseia no `dune-netlist` e
simulam no motor contra forma fechada (RC a 5 mV de 5(1−e^(−t/τ));
RLC a 1e-6 de 1/(1−ω²LC+jωRC); retificador e amplificador por
faixas; op-amp inversor no ganho de A finito a 1e-5; tline com
Γ=0.5 em 6 instantes; bandpass vs R/(R+j(ωL−1/ωC)) a 1e-6; CMOS
rail-to-rail). O `ltraline` tem testes inline no `extract.rs`
(cartão O + `.model` LTRA) e preview visual, sem figura de corpus
própria. 3ª leva (D-0026): `vcamp` (vcvs+vccs, ganhos vin/2, 5·vin
e −10·vin a 1e-9), `ccprobe` (cccs/ccvs exatos), `tuned` (dois
tanques LC com `coupling`, kQ=3.2 sobreacoplado — picos 481/529 kHz
com |H|≈0.5, forma fechada nodal 2×2 a 1e-6), `mos4bias` (nmos4 com
VBS=−2: Id=0.38814 mA pela fórmula de mão a 1e-6) e **diffpair/
rtlinv** — as figuras dos decks de Berkeley, cuja simulação bate
**bit a bit** (diferença 0) com os decks originais; candidatas
óbvias a entrar no `--ci` quando ganharem goldens ngspice próprios.
Previews visuais revisados (regra do
CLAUDE.md). 4ª onda (issue #5): 13 figuras novas — divider,
depsrc (E/G/F/H), dc2src, acrlc, lck, truic, trrlc, trpwl,
acdiode, dfwd, jfet_amp, mos3_inv, rca3040 (11 BJTs) — TODAS
simulando bit a bit como o deck original (helper
`same_as_deck(nome, deck, mapa)` compara toda coluna de todo
plot, complexas em re/im, exigindo worst==0; mapeamentos nos
cabeçalhos dos .pic). Pega de desenho: figuras grandes precisam
de `maxpswid = 12` — o auto-encolhimento do rpic reduz a
geometria mas não o texto, e os rótulos colidem. Descartados com
razão: bridge (o deck não é ponte), mosamp2/schmitt (grandes
demais). Fora do `--ci` do oráculo nesta leva (75 inalterado) —
entrará quando as figuras ganharem goldens ngspice próprios.
5ª onda (final da issue #5): **mos6inv** (as 4 células NDINV como
`subckt6`; o corpo `.SUBCKT` e os seis `.MODEL` do deck, verbatim, em
`corpus/pic/mos6inv_sub.sub` via `.include` — como o opamp) e
**mes_amp** (`nmes`), ambas bit a bit (worst == 0) contra
`spice3f5/examples/mos6inv.cir` e `regress/mes_amp.cir`. O mos6inv
tem 13 redes de um pino só, de propósito: as saídas internas de cada
estágio (nós 11…43 do deck, impressos pelo `.PRINT`) e a saída final
não carregam nada fora do subcircuito — viram pontos de teste e o
`DanglingPin` (warning) é esperado; o teste exige exatamente essas
13 e nada mais (`check_dangling`). Mapeamento: 1 `In`, 2…5 `N2…N5`,
11…43 `N11…N43`, 100 `Vp`.

## Rótulos `$...$` (texlabels, issue #7)

O rpic tipografa um rótulo **inteiro** entre `$...$` (`"$T_1$"`,
`"$V_{CC}$"`) quando duas coisas valem: a figura liga a extensão
(`texlabels = 1` na fonte — o dune-pic não liga por default) e o
processo registrou um renderer em
`rpic_core::set_math_renderer` (gancho global, primeira chamada
vence). Sem renderer, sem `texlabels = 1`, com `$` no meio do texto
ou com TeX inválido, o rótulo sai **literal** (`$T_1$` no desenho) —
a figura nunca falha por causa disso. Só apresentação: o netlist é o
mesmo nos dois casos (o valor vem cru do call-site; o guard do
display compara o texto, que não muda).

| superfície | `$...$` | por quê |
|---|---|---|
| `dune pic --svg OUT.svg` (dune-cli) | **tipografado** | feature `texlabels` (default) registra o `rpic_render::math::render_math` (RaTeX, glifos KaTeX embutidos como `<path>` num `<svg>` aninhado) no topo do `main` |
| dune-cli com `--no-default-features` | literal | sem `rpic-render` no grafo |
| `dune_pic::render_svg` como biblioteca | literal, a menos que o chamador registre | o dune-pic não depende de `rpic-render` (wasm-friendly, zero deps) |
| `dune-sim` (wasm: `renderPic`, galeria, painel do Studio) | literal | nada registrado; custo medido abaixo |
| pacotes Python/R | — | ainda não têm caminho de figura (`spice_pic`/`spice.pic` não existem); quando ganharem, é uma linha de registro + a mesma feature |

`dune-cli` fixa `rpic-core = "=0.16.1"` igual ao do dune-pic: o
gancho é um `static` do crate, e um segundo `rpic-core` no grafo
teria o seu próprio, que ninguém leria. Testes em
`crates/dune-cli/tests/texlabels.rs`: o SVG da CLI tem um `<svg x=`
por rótulo de math e nenhum literal; o caminho puro (o processo de
teste, que não registra nada — o mesmo código do wasm) mantém o
literal; netlist idêntico; e o SVG de toda figura do corpus sem
`texlabels` é byte a byte o do caminho puro.

**wasm32 (medido 2026-11-07, rpic-render 0.16.1 só com `math`):**
compila para `wasm32-unknown-unknown` sem ajuste e tipografa no Node
(3 fragmentos aninhados na figura do teste). Custo no `.wasm` do
pacote (`build.sh`, `wasm-opt -Oz`): 2 408 952 → 5 553 173 B
(+3,14 MB, 2,3×); gzip -9 856 KB → 2 013 KB (+1,16 MB); brotli 11
620 KB → 1 460 KB. Antes do `wasm-opt`, o acréscimo é ~1,45 MB de
dados (fontes KaTeX, tabelas Unicode/métricas) e ~2,06 MB de código,
dos quais ~0,74 MB é a família `regex` (regex-automata, regex-syntax,
aho-corasick, fancy-regex, regex-lite, puxadas pelo `ratex-parser`),
0,42 MB `ratex-parser`, 0,17 MB `ratex-layout`, 0,12 MB `ttf-parser`.
Não ligado no `dune-sim` nesta leva (decisão pendente; rpic-lang#449).
