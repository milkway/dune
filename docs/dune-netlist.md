# dune-netlist — notas de porte

Contrato: `spice.txt` (§2–§4) e o C em `corpus/spice3f5/src/lib/inp`
(+ `fte/inpcom.c`, `fte/subckt.c`). Filosofia do rpic: entrada hostil
produz diagnóstico com span, nunca pânico, nunca loop infinito.

## O que foi portado fielmente

- **Leitor de deck** (`deck.rs` ← `inp_readall`): título = primeira
  linha não vazia; comentários de linha inteira `*`/`$`/`#`;
  continuação `+` junta com um espaço e sobrevive a comentários
  intercalados; o título nunca é continuável; `.include` só na coluna 1,
  emendado antes de tudo (continuações atravessam arquivos).
- **Tokenização** (`scan.rs` ← `INPgetTok`/`INPgetUTok`): separadores
  ` \t = ( ) ,` todos equivalentes e descartados; token de valor
  cortado pela máquina de sign-state do C (quebra em `* / ^` e em
  `+`/`-` interiores que não são sinal nem expoente).
- **Números** (`scan.rs` ← `INPevaluate`): `[±]dig[.dig][(e|d)[±]dig]
  [escala][lixo]`; escalas `t g meg k mil m u n p f` (`mil` multiplica
  por 25,4e-6); letras após número ou escala ignoradas (`10kOhm`);
  `.` e `1.` são números; `1:2` não é (hack de nó de subcircuito do C);
  aritmética via `pow` para cair nos mesmos doubles do C.
- **Cartões de elemento** (`elem.rs` ← `inp2?.c`): todas as 22 letras
  do 3f5 (`R C L K V I E F G H D Q M J Z S W T O U B X`). Parâmetros
  keyword no esquema `INPdevParse`/`INPgetValue`: valor-líder opcional
  e cada keyword engole os números consecutivos que seguem (nenhum →
  flag, um → valor, vários → vetor). O 4º token do BJT desambigua
  substrato×modelo pela tabela de modelos, como `INP2Q` (daí o passo 1
  de coleta de nomes de `.model`, como `inppas1.c`).
- **Cartões ponto** (`dot.rs` ← `inp2dot.c`): despacho por igualdade
  exata do keyword; `.ic`/`.nodeset` leem `v(n)=val` como os tokens
  `v n val`; `.noise` idem para `v(out,ref)`; `.temp` e `.width`
  recebem os mesmos avisos de obsoleto do C; `.print/.plot/.save/
  .four` são parseados para a IR (no 3f5 viviam no frontend,
  `fte/dotcards.c`); `PRTYPE` aceita também `pz` (os decks `pz*.cir`
  do upstream usam `.print pz all`).
- **`.ends nome` divergente é aviso, não erro**: `ltra_3.cir` do
  upstream traz `.ends lump` fechando `.subckt rlclump`.
- Parâmetros de modelo podem ser flags (o LTRA tem `nocontrol`,
  `steplimit`, `quadinterp`); a validação por tabela de cada modelo é
  do `dune-core`.

## Diferenças deliberadas (D-0002: superconjunto pragmático do ngspice)

Registradas também em `corpus/KNOWN_DIFFS.md` quando observáveis:

1. **Expansão de subcircuitos com `.`** (`subckt.rs`): dispositivo
   `r1` em `x1` vira `r.x1.r1` (letra do tipo primeiro, como o ngspice;
   o 3f5 usava `r:x1:r1`); nó interno vira `x1.n1`; portas herdam os
   nomes do chamador; terra fica `0`; referências a dispositivos
   (indutores do `K`, fonte controladora de `F/H/W`) e modelos locais
   são renomeados junto. Motivo: o oráculo compara nomes de vetores
   com o ngspice (D-0006). A expansão é semântica (na IR), não
   textual; profundidade ≤ 64 e ≤ 2²⁰ elementos (bomba de subcircuito
   vira diagnóstico).
2. **Comentários de fim de linha** `$`/`;` precedidos de branco
   (ngspice; o 3f5 só tem comentário de linha inteira). Título nunca
   é cortado.
3. **`.include` relativo ao arquivo que inclui** (o 3f5 usa o diretório
   corrente do processo); profundidade ≤ 16 e orçamento total de
   256 MiB; o loader recusa o que não for arquivo regular.
4. **R/C semicondutores**: um token nu na posição do modelo que seja
   um keyword conhecido (`w l temp ic resistance capacitance area
   off`) é tratado como parâmetro, não como nome de modelo. O C cru
   aceitaria `w` como modelo e depois leria o número como resistência
   — comportamento que o próprio ngspice corrigiu.
5. **CRLF tolerado** (decks DOS).
6. **`.param` básico** (`param.rs`, extensão ngspice, PLAN Fase 1):
   constantes numéricas globais; valores e referências `{…}` aceitam
   números com escala, nomes já definidos e `+ - * / ^` com menos
   unário (`-2^2 = -4`, associatividade à direita do `^`). **Sem**
   parênteses, espaços ou vírgulas dentro de `{}` (viram separadores
   no léxico) e sem funções — amplia-se quando o corpus pedir.
7. **Fonte `B`**: a expressão fica como texto cru na IR
   (`inpptree.c` fica para quando um circuito do corpus usar `B`).

## Pendências conhecidas

- `.lib` diagnostica "não suportado" (nenhum deck do corpus usa).
- `.plot` ignora as faixas `(PLO, PHI)` (sem plotter ainda).
- Variáveis de saída com espaços internos (`v ( 1 )`) viram `Raw` com
  aviso — o frontend do 3f5 tinha um lexer próprio para isso.
- Fuzzing (`cargo fuzz`) entra junto com a estabilização do crate
  (princípio 4 do CLAUDE.md).
