# dune-netlist — notas de porte

Contrato: `spice.txt` (§2–§4) e o C em `corpus/spice3f5/src/lib/inp`
(+ `fte/inpcom.c`, `fte/subckt.c`). Filosofia do rpic: entrada hostil
produz diagnóstico com span, nunca pânico, nunca loop infinito.

## O que foi portado fielmente

- **Leitor de deck** (`deck.rs` ← `inp_readall`): título = primeira
  linha não vazia; comentários de linha inteira `*`/`$`/`#`;
  continuação `+` junta com um espaço e sobrevive a comentários
  intercalados; o título nunca é continuável; `.include` emendado antes
  de tudo (continuações atravessam arquivos). Desde 2026-11-14 também
  `.inc`, diretiva indentada e nome entre aspas (ngspice; o 3f5 só
  conhecia `.include` na coluna 1, sem aspas) e as seções `.lib` — ver
  a seção própria abaixo.
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
  `fte/dotcards.c`); o `.four` exige frequência positiva e finita
  (erro com span) e é executado pela camada `four` do `dune-core`
  (`docs/dune-four.md`); `PRTYPE` aceita também `pz` (os decks `pz*.cir`
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
3. **`.include`/`.lib` relativos ao arquivo que inclui** (o 3f5 usa o
   diretório corrente do processo; o ngspice-47 tenta o diretório
   corrente, depois o do deck, e só então o do arquivo que inclui);
   aninhamento ≤ 16 contando `.include` e `.lib` juntos, ciclo
   detectado com span, 256 MiB lidos e 256 MiB emendados; o loader
   recusa o que não for arquivo regular.
4. **R/C semicondutores**: um token nu na posição do modelo que seja
   um keyword conhecido (`w l temp ic resistance capacitance area
   off`) é tratado como parâmetro, não como nome de modelo. O C cru
   aceitaria `w` como modelo e depois leria o número como resistência
   — comportamento que o próprio ngspice corrigiu.
5. **CRLF tolerado** (decks DOS).
6. **`.param` básico** (`param.rs`, extensão ngspice, PLAN Fase 1):
   constantes numéricas globais; valores e referências `{…}` aceitam
   números com escala, nomes já definidos e `+ - * / ^` com menos
   unário (`-2^2 = -4`; `^` associa **à esquerda**, `2^3^2 = 64`, e
   aceita expoente com sinal, `2^-1 = 0,5` — medido no ngspice-47,
   deck `regress/param_pow.cir`; até 2026-11-14 o Dune associava à
   direita e dava 512). **Sem** parênteses, espaços ou vírgulas dentro
   de `{}` (viram separadores no léxico) e sem funções — amplia-se
   quando o corpus pedir. `.params` é sinônimo (ngspice). Valor não
   finito: um `.param` pode valer `inf`/NaN e passar por outros
   `.param` sem erro (`{1/z}` volta a 0), mas o valor `{…}` que chega a
   um elemento, `.model` ou cartão de controle e não é finito vira
   erro com o span do `{…}` — o ngspice-47 aborta nesse ponto ("unknown
   parameter (inf)") e roda normalmente se o parâmetro não é usado.
   Diferenças: no `.model` o ngspice só avisa e segue com valor
   mutilado, e num subcircuito não instanciado ele nem avalia (o Dune
   avalia o corpo de todo subcircuito até a G3 do `docs/pdk-parser.md`).
   Parâmetros na linha `X` (`x1 a b inv w=1u`, `params:`) ainda não
   existem: erro preciso com o span do nome, em vez do enganoso "no
   such subcircuit `1u`".
7. **Fonte `B`**: ver a seção própria abaixo (expressões com as
   extensões mínimas do ngspice onde o 3f5 rejeita ou mutila).

## Bibliotecas `.lib` (`deck.rs` ← `expand_section_ref` do ngspice-47)

Extensão do ngspice (não existe no 3f5), necessária para ler as
bibliotecas de modelos dos PDKs abertos (G1 de `docs/pdk-parser.md`).
Seguimos o `frontend/inpcom.c` do ngspice-47 (Modified BSD) e sondas
no binário; tudo acontece no leitor, sobre linhas físicas, antes da
junção de continuações:

- `.lib ARQ SEÇÃO` emenda só o corpo da seção `.lib SEÇÃO` … `.endl`
  de `ARQ`; o resto do arquivo nunca é lido como cartão (texto fora de
  seção, inclusive lixo, é ignorado). Nome de seção case-insensitive;
  vale a **primeira** definição do nome; `.endl NOME` divergente é
  aviso (o ngspice aceita calado). Arquivo e seção entre `"…"`, `'…'`
  ou nus; diretivas indentadas e em maiúsculas; `;`/`$` isolado
  começam comentário.
- Dentro da seção, `.lib`/`.include` resolvem relativos ao arquivo da
  biblioteca; a mesma seção pedida duas vezes é emendada duas vezes
  (inofensivo, como no ngspice); outra seção do mesmo arquivo pode ser
  pedida de dentro de uma seção.
- Cada arquivo é lido **uma vez** por deck (cache por caminho
  normalizado lexicamente), e o índice de seções é construído numa
  passada linear na primeira consulta — o gf180 pede o mesmo arquivo
  de 1,3 MB oito vezes.
- Diagnósticos com span: seção inexistente (span no nome da seção, no
  arquivo que pede), `.endl` faltando (span no `.lib SEÇÃO` dentro da
  biblioteca), `.endl` sem `.lib`, `.lib` sem argumentos, aspas sem
  fechamento, nome vazio, arquivo inexistente/diretório/ilegível. Um
  `.lib NOME` (definição) fora de biblioteca é erro e o bloco até o
  `.endl` é pulado (o ngspice para em "unimplemented dot command");
  definição aninhada dentro de seção é erro, e a seção termina no
  primeiro `.endl`, como no ngspice. Erros dentro de uma seção apontam
  para a linha da biblioteca.
- Entrada hostil: aninhamento ≤ 16 contando `.include` e `.lib` juntos;
  ciclo (o mesmo arquivo, ou a mesma seção do mesmo arquivo, já na
  pilha) é erro com o span da diretiva que fecha o ciclo — o ngspice-47
  dá *segfault* num ciclo de `.include` e um erro espúrio de aspas num
  ciclo de `.lib`; 256 MiB de texto **lido** (cada arquivo uma vez) e
  256 MiB **emendados** (uma seção pedida milhares de vezes, ou um
  leque de aninhamentos, para com um erro só — tudo para a partir
  dali).
- Diferenças medidas (linhas no `KNOWN_DIFFS.md`): caminho só relativo
  ao arquivo que inclui (o ngspice procura antes no diretório corrente
  e no do deck); `.include` de seções **não pedidas** não é lido (o
  ngspice lê todos os `.include` do arquivo ao carregá-lo, e um
  inexistente numa seção não usada é fatal lá).
- No WASM não há sistema de arquivos: `.lib`/`.include` passam pelo
  mesmo `IncludeLoader` do mapa `includes` das opções; arquivo fora do
  mapa vira o diagnóstico "cannot open library file …: not among the
  supplied includes".

## Expressões da fonte `B` (`expr.rs` ← `inpptree.c`)

O cartão `Bxxx N+ N- V=EXPR` / `I=EXPR` segue o `INP2B` +
`INPdevParse`: um número líder é ignorado (aviso); depois um laço de
keywords — `v` ou `i`, o `INPgetTok` engole brancos/`=`/`,`, e o
`INPgetTree` lê a expressão **até onde ela vai**; a varredura do
cartão continua dali. O último `v=`/`i=` vence (`ASRCparam`
sobrescreve tipo e árvore); qualquer outro keyword é "unknown
parameter" e encerra o cartão. A IR guarda a árvore tipada
(`ir::Expr`: `Num`, `V(nó)`, `I(dispositivo)`, `Bin(op)`,
`Call(função)`), sem dobra de constantes — só as árvores de derivada
dobram, no `dune-core`.

Gramática (porte do `PTparse`, precedência por tabela
`prectable[11][11]` copiada do C, pilha de 200 como `PT_STACKSIZE`):

- `+ -` (associam à esquerda) < `* /` < `^` (associa à direita) <
  menos unário. **Não há `+` unário** (`+1` é erro de sintaxe, como
  no C) e `--1` também é erro (`u-` seguido de `u-` reduz sem
  operando); `-(-1)` funciona. `-2^2` é `(-2)^2`. O ngspice-47 difere
  nos dois pontos (sonda de 2026-11-14: `v=2^3^2` dá 64 e `v=-2^2` dá
  −4 lá, 512 e 4 aqui); fica o 3f5 (linha no `KNOWN_DIFFS.md`) — a
  linguagem do `.param` (acima) segue o ngspice.
- `nome(expr)`: as 20 funções do `funcs[]` — `abs acos acosh asin
  asinh atan atanh cos cosh exp ln log sgn sin sinh sqrt tan tanh u
  uramp` (semântica do `ptfuncs.c` no `dune-core`: `log` é base 10;
  `ln`/`log`/`sqrt` tomam `|x|`; `u` é o degrau com ½ em 0).
- `v(n)`, `v(n1,n2)` (= `v(n1) - v(n2)`, recursivo: `v(a,b,c)` é
  `v(a)-v(b)-v(c)` como no `mkfnode`), `i(vnome)`.
- Constantes `e` e `pi`; números pelo `INPevaluate` sem gobble.
- O lexer termina a expressão onde um valor seguiria outro valor ou
  um `)` (`v(1) i=3`, `2 3`) e num `(` depois de número (`2(3)`).

Diferenças deliberadas (todas em entradas que o 3f5 rejeita ou
mutila; linhas no `KNOWN_DIFFS.md`):

- **Fator de escala consumido** (`2k*v(1)` = 2000·v(1)): o lexer do
  3f5 (`INPevaluate` com gobble 0) para *sobre* o `k`, a expressão
  termina ali e o `k` vira "unknown parameter". Superconjunto, como o
  ngspice.
- **Nomes dentro de `v(…)`/`i(…)`** vão até branco, `,`, `(` ou `)`,
  como no lexer do ngspice: `v(1a)`, `v(n-1)`, `v(x1.n2)` funcionam.
  No 3f5 o argumento passava pelo lexer geral: `v(1a)` era erro,
  `v(1.0)` virava `v(1)` (`sprintf("%d")`) e `v(e)`/`v(pi)` viravam
  `v(2)`/`v(3)` (a constante!).
- **`time`**: o 3f5 aceita o sinal especial no parser
  (`specSigList`), mas o `ASRCsetup`/`ASRCload` não têm caso para a
  variável `IF_STRING` e a carga devolve `E_BADPARM`; aqui é
  diagnóstico com span já no parse. As fontes B dependentes do tempo
  do ngspice não foram portadas.
- **Limites**: profundidade ≤ 512 e ≤ 10 000 nós por árvore
  (`EXPR_MAX_DEPTH`/`EXPR_MAX_NODES`), para que as passagens
  recursivas (derivada, avaliação, renomeação) não estourem a pilha
  com entrada hostil; o C só tinha a pilha de 200 do parser, que
  não limita somas longas.
- **Subcircuitos**: os nós de `v()` e os dispositivos de `i()` são
  renomeados com o cartão (`v(n)` → `v(x1.n)`, `i(vx)` →
  `i(v.x1.vx)`), como o `subckt.c` do ngspice; o frontend do 3f5 só
  traduzia os dois nós do cartão.
- Duplicatas de `i(x)` viram uma variável só (o C compara um
  `temp.uValue` não inicializado e duplica; mesma soma no Jacobiano).

Ficou de fora (extensões do ngspice além do 3f5): `time`, `temper`,
`hertz`; operadores relacionais/lógicos e o ternário `?:`; `**`;
as funções `log10 ceil floor nint u2 pwl pow pwr min max ddt eq0 ne0
gt0 lt0 ge0 le0`; os parâmetros de instância `tc1 tc2 temp dtemp m
reciproctc reciprocm`; `{}`/`.param` dentro da expressão.

Erros: toda expressão malformada vira diagnóstico com o span do
trecho (`syntax error`, `no such function`, `badly formed node
voltage`/`branch current` — as mensagens do C —, nome solto,
vírgula fora de `v()`, limites). Fuzz: o alvo `parse_deck` também
alimenta `parse_expr` e um cartão B com os bytes; o smoke
determinístico (`tests/fuzz_smoke.rs`) muta os cartões B das sondas
`bsrc_*` e leva o parser aos limites.

`dune_netlist::parse_expr(texto)` expõe o mesmo parser (expressão
inteira; texto sobrando é erro).

## Pendências conhecidas

- `.plot` ignora as faixas `(PLO, PHI)` (sem plotter ainda).
- Variáveis de saída com espaços internos (`v ( 1 )`) viram `Raw` com
  aviso — o frontend do 3f5 tinha um lexer próprio para isso.
- Fuzzing (`cargo fuzz`) entra junto com a estabilização do crate
  (princípio 4 do CLAUDE.md).
