# dune-pic — extrator de netlist de figuras rpic

Fase 5 (PLAN §2.5; D-0003/D-0004/D-0019/D-0020). Depende de
`rpic-core = "=0.12.1"` (crates.io, zero deps, wasm-friendly) — só
API pública, como o D-0003 manda.

## Pipeline

1. **compat (interino)**: `strip_directives` apaga (preservando
   contagem de linhas) toda linha cujo primeiro caractere não-branco
   é `.` fora de `.PS/.PE`, guardando-as como diretivas com span
   exato — espelho do `strip_backend_preamble` do rpic (W1 do
   D-0020).
2. Compila prelude (`corpus/pic/spice.pic`, embutido) + figura com o
   rpic (std `circuits: true`; `copy` negado). Exige `spice = 1` nos
   `vars`.
3. Colhe blocos rotulados (label = refdes; 1ª letra = cartão, só
   R C L V I D Q M J S W X), marcadores de pino invisíveis, blocos
   `ground()` (nó 0) e `Nome: node(P)` (nomes de nó — W2, interino),
   fios (todo vértice de polilinha é terminal) e `dot`s.
4. **Conectividade**: union-find com `snap` (default 1e-3"), junção
   em T (terminal no interior de segmento conecta — regra nossa;
   cruzamento puro NÃO conecta, `dot` resolve), `node()` iguais
   unificam como net labels.
5. **Nomes**: `0` do ground, depois `node()`, depois `n1, n2…` por
   menor índice de shape (determinístico).
6. **Emissão**: netlist SPICE3 textual (valor/modelo = primeiro
   shape de texto do bloco, verbatim) + diretivas na ordem da fonte
   (+ continuações `+`; `.end` do usuário é deduplicado) — passa
   pelo `dune-netlist` normal. `OverlayMap` dá refdes→`sN..sM` e
   net→shapes para a sobreposição do Studio (Fase 6).
7. **Diagnósticos** com span da figura: pino solto, net flutuante,
   valor faltando (warning em V/I — SPICE trata como 0 — e erro no
   resto), refdes duplicado, letra desconhecida, sem `spice = 1`,
   conflito de nome de nó.

## Convenções pic (D-0020)

- Pinos: 2-terminais `P`/`N` (P = 1º ponto e 1º nó: + de fonte,
  anodo, cauda da seta); Q `C/B/E`; J `D/G/S`; M `D/G/S/B` (B
  desenhado amarrado ao S); X `Inp/Inm/Out`; S `P/N/Cp/Cn`; W
  `P/N` com valor `Vctl modelo`.
- Labels pic exigem maiúscula (por isso não há `R1.a`);
  `Gnd`/`Node`/`At` são reservados pelo prelude.
- Macros do prelude: resistor, capacitor, inductor, diode, vsource,
  acsource, battery, isource, switch, cswitch, npn, pnp, nmos,
  pmos, njfet, pjfet, opamp, ground, node.
- Valor desenhado na figura (acima do elemento horizontal, à
  esquerda do vertical) — é o que o extrator lê.

## Interinos e o que troca com o rpic-core pós-PRs

Tudo em `crates/dune-pic/src/compat.rs`, marcado:

| Interino | Substituto (PRs do rpic-lang) |
|---|---|
| W1 `strip_directives` | `Drawing.passthrough` |
| W2 `node()` (+ `NODE_MARKER`/`BlockKind::Node`) | `Drawing.places` |
| W3 `call_site` (acha o call-site escaneando o n-ésimo `Label:`) | `BlockInfo.span` |

Depois: bump do pin `=0.12.1` para a release com os PRs.

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
  (linhas de corpo não começam com `.`) — precisa de `.include`;
  sem teste de simulação por ora.
- O `snap` se aplica depois do auto-scale do rpic (`maxpswid`);
  prelude põe `margin = 0.1`.

## Corpus e validação

`corpus/pic/{rc,rlc,rect,bjtamp}.pic` + goldens revisados à mão em
`corpus/pic/golden/*.cir`; testes comparam o texto extraído, provam
que parseia no `dune-netlist` e simulam no motor contra forma
fechada (RC a 5 mV de 5(1−e^(−t/τ)); RLC a 1e-6 de
1/(1−ω²LC+jωRC); retificador e amplificador por faixas). Previews
visuais revisados (regra do CLAUDE.md). Fora do `--ci` do oráculo
nesta leva (75 inalterado) — entrará quando as figuras ganharem
goldens ngspice próprios.
