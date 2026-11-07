# dune-pic — extrator de netlist de figuras rpic

Fase 5 (PLAN §2.5; D-0003/D-0004/D-0019/D-0020/D-0021). Depende de
`rpic-core = "=0.13.0"` (crates.io, zero deps, wasm-friendly) — só
API pública, como o D-0003 manda. Os três interinos do D-0020
foram aposentados quando os PRs #414/#415/#416 do rpic-lang saíram
na release 0.13.0.

## Pipeline

1. **Diretivas**: vêm de `Drawing.passthrough` (linhas `.` de
   **coluna 1**, regra do dpic real — indentado é erro de compile,
   testado). O `compat` restante só blanka e re-anexa as
   continuações `+` (o pic erra nelas) e fatia os argumentos crus
   do call-site.
2. Compila prelude (`corpus/pic/spice.pic`, embutido) + figura com o
   rpic (std `circuits: true`; `copy` negado). Exige `spice = 1` nos
   `vars`.
3. Colhe blocos rotulados (label = refdes; 1ª letra = cartão, só
   R C L V I D Q M J S W X T O E F G H K — o K não tem pinos, só o
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
   argumento do call-site relido CRU da fonte do usuário no
   `BlockInfo.span` (o `MacroCall.args` do 0.13 vem normalizado —
   `1e-9` viraria `0.000000001`; ver D-0021) — com fallback para o
   texto desenhado quando não há call de uma linha só ou um macro
   embrulhador intercepta (guardado por comparação pós-normalização,
   testado). O prelude segue desenhando o valor (apresentação).
   Diretivas na ordem da fonte
   (+ continuações `+`; `.end` do usuário é deduplicado) — passa
   pelo `dune-netlist` normal. `OverlayMap` dá refdes→`sN..sM` e
   net→shapes para a sobreposição do Studio (Fase 6).
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
  desenhado amarrado ao S); X `Inp/Inm/Out`; S `P/N/Cp/Cn`; W
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
  njfet, pjfet, opamp, tline, ltraline, ground. (O `node()`
  interino saiu — nomes de nó são places.)
- Valor desenhado na figura (acima do elemento horizontal, à
  esquerda do vertical) — é o que o extrator lê.

## Display (rótulo curto no desenho)

- Argumento opcional **depois** do valor/modelo: quando não vazio, o
  prelude desenha o display no lugar do valor —
  `vsource(In, (0,0), PULSE(0 5 0 1u 1u 10m 20m), "G₁")` desenha só
  "G₁"; o cartão continua com o PULSE completo. Aceitam display: os
  2-terminais com valor (R C L D, v/ac/i-source, battery, cccs,
  ccvs), vcvs/vccs (6º argumento) e os centrados em `At`
  (npn/pnp/npn4/pnp4, nmos/pmos/nmos4/pmos4, njfet/pjfet, opamp).
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

## O que restou do `compat` (pós-0.13)

- `split_continuations`: o pic erra em linhas `+` de coluna 1, então
  elas são blankadas e re-anexadas à diretiva da linha de cima.
  Sairia se o passthrough do rpic um dia englobasse continuações.
- `raw_call_args`: fatia `name(…)` por vírgulas de topo (respeitando
  colchetes/strings) para ler argumentos crus no span — sairia com
  um campo raw-args no rpic (candidato a PR 4, ver D-0021).

## Ressalvas conhecidas

- Regra de junção em T é interpretação nossa (ferramentas de
  esquemático fazem assim; endpoint-only tornaria `dot` no meio de
  fio inútil).
- O prelude carrega cópias verbatim da geometria do std
  (switch/nmos/pmos/njfet/pjfet/battery, BSD-2) porque macros flat
  não aceitam label — podem derivar se o std mudar.
- `pmos`: o std chama `gDrain` o terminal de cima; o prelude mapeia
  S e B em cima e D embaixo (convenção de símbolo PMOS).
- `vsource` chama `voltage_source($2,$1)` invertido para o `+`
  encarar o P.
- `PRELUDE` usa `include_str!` com caminho fora do crate
  (`corpus/pic/spice.pic`) — `cargo package` do dune-pic falharia;
  irrelevante enquanto não publicamos o crate.
- Op-amp emite `X` mas o corpo `.subckt` não cabe em linhas `.`
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
- A releitura crua assume colunas em caracteres e confere a
  contagem de argumentos contra a do rpic antes de usar.
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
CLAUDE.md). Fora do `--ci` do oráculo nesta leva (75 inalterado) —
entrará quando as figuras ganharem goldens ngspice próprios.
