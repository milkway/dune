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
  `v n val`; `.noise` idem para `v(out,ref)`; `.width` recebe o
  mesmo aviso de obsoleto do C; `.temp` segue o ngspice-47 (seção
  abaixo); `.print/.plot/.save/
  .four` são parseados para a IR (no 3f5 viviam no frontend,
  `fte/dotcards.c`); o `.four` exige frequência positiva e finita
  (erro com span) e é executado pela camada `four` do `dune-core`
  (`docs/dune-four.md`); `PRTYPE` aceita também `pz` (os decks `pz*.cir`
  do upstream usam `.print pz all`).
- **`.ends nome` divergente é aviso, não erro**: `ltra_3.cir` do
  upstream traz `.ends lump` fechando `.subckt rlclump`.
- **Linhas KSPICE do ngspice-47** (`elem.rs::txl`, `cpl`): `Y N1 GND1
  N2 GND2 MNAME <LEN v>` guarda os quatro nós e separa `len` (o que o
  `INP2Y` lê) de `length` (o que o `INP2P` lê quando o modelo é CPL);
  `P` conta os tokens até `length`/`len`, como o `INP2P`, e exige `2k +
  3` nomes (`k` entradas, referência, `k` saídas, referência, modelo)
  com `1 ≤ k ≤ 8`; lixo depois do modelo ou do comprimento é erro (o C o
  ignora). Tipos de modelo `txl` e `cpl` (`docs/kspice-lines.md`).
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
   detectado com span, 256 MiB lidos, 256 MiB percorridos, 100 000 entradas e 1000 diagnósticos do leitor; o loader
   recusa o que não for arquivo regular.
4. **R/C semicondutores**: um token nu na posição do modelo que seja
   um keyword conhecido (`w l temp ic resistance capacitance area
   off`) é tratado como parâmetro, não como nome de modelo. O C cru
   aceitaria `w` como modelo e depois leria o número como resistência
   — comportamento que o próprio ngspice corrigiu.
5. **CRLF tolerado** (decks DOS).
6. **`.param` e expressões de parâmetros** (`pexpr.rs`, `param.rs`,
   extensão ngspice): a linguagem inteira do `.param`/`{…}`/`'…'` com a
   semântica observada no ngspice-47 — ver a seção própria abaixo
   ("Linguagem de expressões de parâmetros"). `.params` é sinônimo.
   Valor não finito: um `.param` pode valer `inf`/NaN e passar por
   outros `.param` sem erro (`{1/z}` volta a 0), mas o valor que chega
   a um elemento, `.model` ou cartão de controle e não é finito vira
   erro com o span — o ngspice-47 aborta nesse ponto ("unknown
   parameter (inf)") e roda normalmente se o parâmetro não é usado.
   Diferença: num `.model` **usado** o ngspice só avisa e segue com
   valor mutilado. Subcircuito nunca instanciado e `.model` não usado
   não são avaliados, como no ngspice (G3, seção "Parâmetros de
   subcircuito" abaixo).
7. **Fonte `B`**: ver a seção própria abaixo (expressões com as
   extensões mínimas do ngspice onde o 3f5 rejeita ou mutila).

## Cartão `.temp` (`dot.rs::dot_temp` ← `inp_spsource` do ngspice-47)

O 3f5 ignorava o cartão ("Warning: .TEMP card obsolete - use .options
TEMP and TNOM", `inp2dot.c`; o `spice.txt` nem o lista). O ngspice-47
o aplica (`frontend/inp.c`, Modified BSD), e o Dune segue o observado
(sondas `temp*.cases` em `.claude/scratch/fix-probes/`):

- `.temp T`, `.temp=T` e `.temp = T` são o mesmo cartão; o valor vira a
  opção `temp` (°C) **depois** de todos os `.options`, então ganha de
  `.options temp=` em qualquer posição (`t_opt_card`, `t_card_opt`), e o
  último `.temp` vence (`t_last`). Na IR: um `Param` `temp` no fim de
  `Netlist::options`, com o span do cartão.
- Expressões `{…}`/`'…'` valem (`t_expr`, `t_quote`); nome solto não
  (`t_param_bare`).
- O resto da linha precisa ser **um** número de `strtod`: sem sufixo de
  escala (`0.127k`), sem lista (`77 127`, `77, 127`). Fora disso o
  ngspice avisa "Could not set temperature to …" e roda a 27 °C,
  passando por cima de `.options temp` (`t_multi`, `t_multi_opt`,
  `t_text`); o Dune faz o mesmo com aviso com span que diz que lista
  não é varrida (nem o ngspice reroda por temperatura).
- `TNOM`, `TEMP=`/`DTEMP=` de instância e `.dc temp` mantêm o sentido
  (`t_tnom`, `t_itemp`, `t_dtemp`, `t_dc`: a varredura manda durante o
  `.dc` e o `.temp` volta depois).
- Recusados (o ngspice segue): cartão vazio (`strtod("")` dá 0 °C,
  `t_empty`), valor não finito (`inf` faz o op falhar em todos os
  degraus, `t_inf`) e `.temp` dentro de `.subckt` (o ngspice o aplica ao
  circuito inteiro quando o subcircuito é instanciado e o ignora quando
  não é, `t_subckt`, `t_sub_unused`): erro com span pedindo o nível de
  topo.

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
  ciclo de `.lib`; e quatro orçamentos, cada um parando a leitura com
  um erro só (com span) — tudo para a partir dali:
  - 256 MiB de texto **lido** (`MAX_READ_BYTES`; cada arquivo uma vez);
  - 256 MiB de texto **percorrido** (`MAX_VISITED_BYTES`): cada entrada
    num arquivo ou seção (`.include`, `.lib`, o deck raiz) cobra o
    tamanho inteiro do que percorre — diretivas, comentários e linhas
    em branco incluídos —, esteja o arquivo em cache ou não; um
    arquivo só de comentários incluído milhares de vezes para aqui;
  - 100 000 **entradas** (`MAX_ENTRIES`): um leque de seções só de
    diretivas (cada seção pedindo outras quatro, 15 níveis: 4¹⁵
    entradas sem limite) ou um arquivo vazio incluído sem fim para
    aqui; o inversor sky130 usa 170;
  - 1000 **diagnósticos** do leitor (`MAX_READER_DIAGS`), depois um
    erro final; falha de abertura fica em cache (o loader é chamado
    uma vez por caminho).
  Os limites saíram de uma revisão e de um *timeout* do libFuzzer
  (13 s, entrada em `tests/fixtures/fuzz/lib-fanout-timeout.bin`, hoje
  18 ms).
- Diferenças medidas (linhas no `KNOWN_DIFFS.md`): caminho só relativo
  ao arquivo que inclui (o ngspice procura antes no diretório corrente
  e no do deck); `.include` de seções **não pedidas** não é lido (o
  ngspice lê todos os `.include` do arquivo ao carregá-lo, e um
  inexistente numa seção não usada é fatal lá).
- No WASM não há sistema de arquivos: `.lib`/`.include` passam pelo
  mesmo `IncludeLoader` do mapa `includes` das opções; arquivo fora do
  mapa vira o diagnóstico "cannot open library file …: not among the
  supplied includes".

## Linguagem de expressões de parâmetros (`pexpr.rs`, `param.rs`)

G2 de `docs/pdk-parser.md` (2026-11-14). É a linguagem do `.param`,
do `{…}` e do `'…'` — a "numparam" do ngspice, que o 3f5 não tem —,
**separada** da gramática da fonte `B` (seção seguinte, porte do
`inpptree.c`, que fica no 3f5).

**Proveniência.** O contrato é o comportamento do `ngspice` 47
(Homebrew) observado em decks de sonda, mais o manual. O código do
ngspice para esta linguagem (`src/frontend/numparam/`, LGPL) **não**
foi aberto. Do `frontend/inpcom.c` (Modified BSD) foi lido só o
pré-processamento de linhas: `inp_change_quotes` (`'…'` → `{…}`),
`inp_remove_ws`, `inp_split_multi_param_lines`, `inp_fix_param_values`
(valor nu depois de `=` ganha `{}`), `inp_sort_params` (ordem de
dependência, "último vence") e `inp_casefix` (aspas duplas somem).
As sondas — cerca de 490 decks mínimos, com a saída do ngspice —
estão em `.claude/scratch/g2-probes/` (`probe.py CASOS DIR` gera e
roda; `cN.txt` são os casos, `cN/` os decks e logs, `cN_result.txt` o
resumo); cada regra abaixo cita a sonda (`c2 t_nest1` = caso
`t_nest1` de `c2.txt`) e cada teste de `pexpr.rs`/`param.rs`/
`tests/param_expr.rs` cita a sua.

### Gramática e precedência

Da mais frouxa para a mais forte:

| Nível | Operadores | Associação | Sonda |
|---|---|---|---|
| 1 | `c ? a : b` | direita (`0?2:0?4:5` = 5); menor de todas (`1+1?2:3` = 2, `2*1?3:4` = 3) | c1 p_tern_*, c2 t_* |
| 2 | `\|\|` | esquerda | c1 p_and_or |
| 3 | `&&` | esquerda, mais forte que `\|\|` (`1\|\|1&&0` = 1, `0&&1\|\|1&&1` = 1) | c2 c_or_and3, c_and_or_lvl |
| 4 | `== = != <> < > <= >=` | **um só nível**, esquerda (`2==1<3` = 1, `1==2<1` = 1, `3>2>1` = 0) — diferente do C | c2 c_eq_lt, c8 v_eq_lt2, c1 p_cmp_chain_lt |
| 5 | `+ -` binários | esquerda | c1 p_sub_left |
| 6 | `* / %` | esquerda, um nível (`2*5%3` = 1, `5%3*2` = 4) | c8 v_mul_mod, v_mod_mul |
| 7 | sinal unário (só no início de operando, ver abaixo) | — | c8 v_neg_* |
| 8 | `^` e `**` | **esquerda** (`2^3^2` = 64) | c1 p_pow_left, P05 |
| 9 | `!` prefixo | mais forte que `^` (`!2^0` = (!2)^0 = 1) | c2 u_not_pow |
| 10 | número, nome, `f(args)`, `( )`, `{ }`, `' '` | — | |

- **Sinais.** No início da expressão, depois de `(` e depois de `,` o
  sinal é operador unário, mais fraco que `^` (`-2^2` = −4) e mais
  forte que `+`/`*`/comparação (`-2+3` = 1, `-1<0` = 1). Depois de
  **qualquer outro operador** — `?` e `:` inclusive — um `-` precisa
  vir colado a um número e forma um literal negativo, mais forte que
  `^`: `3*-2^2` = 12, `0+-2^2` = 4, `1?-2^2:3` = 4, `0?3:-2^2` = 4,
  `1>-2` = 1, `2*-1k` = −2000 (c1, c2, c8). Nesses lugares o
  ngspice-47 **rejeita** `-` seguido de nome, `(` ou função (`2*-(3)`,
  `1<-a`, `--a`, `2*-abs(3)`) e qualquer `+` (`2*+3`, `1++2`), e
  **lê errado** dois casos (`3*-a^2` = 0,75; `1?-a:3` = 2): todos são
  erro no Dune, com a sugestão `2*(-x)`. `!` seguido de sinal é erro
  (`!-1`, c1 p_not_neg). `--3` = 3 (sinal unário + literal).
- `^`/`**` elevam o **módulo** da base: `(-2)^3` = 8, `(-2)^2.5` =
  5,657, `(-8)^(1/3)` = 2, `0^0` = 1 (c1, c3 f_pow_hat_neg); `%` é o
  `fmod` do C (`7.5%2` = 1,5, `-7%3` = −1, `7%0` = NaN; c2).
- Lógica e comparação valem 0/1; verdadeiro é "≠ 0" (NaN é verdadeiro:
  `z?1:2` = 1 com z NaN; `z==z` = 0; c1 z_nan_*). `=` sozinho compara
  como `==` (`{1=1}` = 1, c1 p_single_eq). `&`, `|`, `~` não existem
  (c1).
- Ternário aninhado no **operando do meio** sem parênteses é erro: o
  ngspice-47 lê `1?0?7:8:9` como 7 (c2 t_nest1..4); `1?(0?7:8):9` = 8
  funciona. `1?2` sem `:` é erro (o ngspice dá 1).
- `{…}` e `'…'` aninhados dentro de uma expressão agrupam como
  parênteses (`{{a}+1}`, `{'a'+1}`, `'{a}+1'`, c6). Nomes: `[A-Za-z_]
  [A-Za-z0-9_]*`, sem diferenciar maiúsculas (`a.b` e `a$b` não são
  nomes, c4). **Não há constantes** embutidas: `pi`, `e`, `true`,
  `boltz`… são nomes indefinidos (c4 i_*). Um nome de função sem
  parênteses é erro (`{sqrt}`), mesmo com um `.param sqrt` (c4).
- Nomes indefinidos são erro mesmo num ramo não tomado do `?:`
  (`1?2:nosuch`, c8 e_untaken_undef); valores não finitos num ramo não
  tomado não importam (`1?2:sqrt(-1)` = 2).

### Números

Mantissa decimal, expoente `e`/`E` só se seguido de dígitos (`2e` = 2,
`2e3k` = 2e6), depois uma sequência de letras cujo começo dá a escala:
`meg` 1e6, senão `t g k m u n p f a` (o **`a` é atto** e **`mil` é
mili** — `1mil` = 1e-3, `1milk` = 1e-3; letras sem escala, como `x`,
`ohm`, `hz`, valem 1). O valor é o decimal escalado arredondado
corretamente (`1.1k` é 1100 exato, `0.3m` é 3e-4 exato; c5 s_*), não
`mantissa×10^k` como o `INPevaluate`. Dígito, `.` ou `_` logo depois das
letras é erro (`2d3`: não há expoente Fortran aqui, c4 n_d3); `.` sozinho
é erro; hexadecimal (`0x10` = 16 no ngspice, que lê com `strtod`) é
recusado — linha no `KNOWN_DIFFS.md`.

### Funções

As 34 que o ngspice-47 tem (c3), todas implementadas: `sqrt abs exp
log ln log10 floor ceil int nint sgn sqr sin cos tan asin acos atan sinh
cosh tanh asinh acosh atanh` (um argumento), `min max pow pwr` (dois),
`ternary_fcn(c,a,b)`, e as aleatórias `agauss(nom,avar,sigma)`,
`gauss(nom,rvar,sigma)` (dois ou três argumentos), `aunif(nom,avar)`,
`unif(nom,rvar)`, `limit(nom,avar)`. Semântica observada: `log` é
**natural** (= `ln`); `pow(-2,3)` = −8 e `pow(-8,1/3)` = NaN (o `pow` do
C), `pwr(-2,3)` = 8 (módulo); `int` trunca (`int(-2.7)` = −2); `nint`
arredonda metade para par (`nint(2.5)` = 2, `nint(3.5)` = 4,
`nint(-2.5)` = −2); `sgn(0)` = 0; fora do domínio dá NaN (`sqrt(-4)`,
`log(-1)`, `acosh(0.5)`). O ngspice-47 **não tem** `round trunc sign
atan2 hypot fabs u uramp if db pwrs random rand mod` (nem `v`, `i`,
`ddt` aqui): "unknown function" (para `v(…)`/`i(…)`, "behavioural
R/C/L not supported yet"). Contagem errada de argumentos é erro; o
ngspice a lê errado (`sqrt(1,2)` = √2, `min(3,1,2)` = 2, `min(3)` = 1,
`pow(2)` = 1, `limit(5,0,3)` = 5±3 — a vírgula extra vira "o último
argumento vence"; c3).

### Valor visto pelo cartão

O ngspice substitui o resultado **como texto** no cartão
(`%.15e`) e o leitor do cartão o relê com o `INPevaluate`: `{7}`
chega ao dispositivo como 7,000000000000001, `{3}` como
3,0000000000000004, `{1n}` como 9,99999999999999855e-10 (`print
@dev[param]` com `numdgt=17`, c7 e c9; 25 valores na emulação
`fmt_emul.py`, que descarta `%.14e`, `%.16e`, `%.17e` e `%.15g`–`%.17g`,
mais 11 da c9 no teste de unidade — todos idênticos). O Dune faz o mesmo
(`pexpr::as_substituted`) em todo valor de expressão que chega a um
elemento, `.model` ou cartão de controle; entre `.param` o valor fica
em `f64` (c2 store_*). Número literal fora de `{}` não muda.

### No cartão

- O `Scanner` mantém um campo `{…}` inteiro (conta aninhamento, como
  `inp_spawn_brace`) e um campo que começa com `'` até o `'` seguinte:
  espaços, vírgulas e parênteses dentro não cortam mais.
- **Valor nu depois de `=`** (`w=wv`, `rsh=rv*2`, `noia=max(a, b)`):
  expressão até o primeiro branco fora de parênteses (ou `,`/`)` fora
  de parênteses), como o `inp_fix_param_values`; não vale em
  `.options` (o ngspice pula essas linhas). Sem `=` um nome nu não é
  parâmetro (`r1 1 0 rm w=2 l=3` com um `.param l` continua lendo o
  keyword `l`); `v1 1 0 p` posicional é diferença registrada.
- Aspas duplas somem das expressões de cartão (o `inp_casefix` as
  apaga em toda linha que não seja `.param`, `.subckt` ou `X`): `{2*"3+1"}`
  = 7, e o sky130 escreve `dw = {"-tol_nfom/2-nfom_dw/2"}` (c10). Em
  `.param` são erro, como no ngspice.

### `.param` e o escopo global

- Forma: `.param nome = valor [,] nome = valor …`, continuação `+`,
  maiúsculas, diretiva indentada, comentário `;`/`$`. O valor é `{…}`,
  `'…'` ou nu até o primeiro branco ou `,` fora de parênteses (c5
  l_*). Texto depois de um valor que não começa outro `nome=` é erro
  (c5: o ngspice rejeita `.param b = a * 3`, mas numa linha com duas
  atribuições **descarta calado** o `* 3` e faz `b = a`). `.param s="…"`
  (string) é erro. Um `.param` dentro de `.subckt` pertence ao escopo
  da instância (G3, seção seguinte) e não vaza para o global.
- Resolução (`param::resolve`): **a última definição de um nome é a
  única que existe**, inclusive para usos anteriores (c5 r_last,
  r_last_dep; `.param c=1` / `.param c={c+1}` é ciclo, c5 r_self);
  referência adiante é legal (c5 r_fwd, r_fwd_chain); ciclo é erro com
  span na referência que o fecha, com o caminho (`a → b → a`; o
  ngspice diz "level depth greater 1000"); nome indefinido, função
  desconhecida ou erro de sintaxe em **qualquer** `.param` global é
  erro, usado ou não (P05e, c5 r_undef_unused); um `.param` que falhou
  não gera erros em cascata nos que dependem dele. A avaliação segue o
  grafo de dependência com pilha explícita, cada definição uma vez.

### Funções aleatórias e `temper`

`agauss gauss aunif unif limit` são lidas inteiras, mas **avaliadas no
valor nominal** (o primeiro argumento) e o deck recebe **um** aviso, no
primeiro uso: o ngspice sorteia a cada execução, sem semente (P06), e
a política do Dune é decisão pendente do dono (G8). O avaliador já tem
o modo sorteado (`RandomPolicy::Seeded`, gerador SplitMix64 + Box–Muller,
fórmulas do manual: `agauss` = nom + avar/sigma·N(0,1), `gauss` relativo,
`aunif`/`unif` uniformes em ±var, `limit` = nom ± avar), alcançável só
nos testes. `temper`: seção própria, abaixo.

### `temper` (G7)

Fase 8 (2026-11-14). No ngspice-47 o `temper` não é um parâmetro do
`numparam`: o `inpcom.c` (Modified BSD; lidos `inp_fix_temper_in_param`
e `inp_temper_compat`) transforma um `.param` que o nomeia em `.func`
sem argumentos e troca os usos dele, **no mesmo nível de subcircuito**,
por chamadas; depois, nas linhas `.model` e nas de dispositivo que não
começam por `V I E G F H B`, reescreve os `{…}` que contêm `temper`
para o parser da fonte B, que os reavalia a cada temperatura. Em
qualquer outro lugar o `numparam` responde "Undefined parameter
[temper]". Sondas: `.claude/scratch/g9-probes/t1.txt` e `t2.txt`
(testes em `tests/temper.rs`; decks `regress/temper_op.cir` e
`temper_dc.cir`).

| Posição | ngspice-47 | Sonda |
|---|---|---|
| valor de `.model` (`{…}`, `'…'`, nu) | a temperatura do circuito | `t1 t_model*`, `t_case` |
| valor de dispositivo R, C, L, D, Q, J, M… (posicional ou palavra-chave) | idem | `t_r_pos*`, `t_c_pos`, `t_diode_area`, `t_mos_w`, `t2 t_l_val` |
| `V`, `I` (posicional, `dc`), `E G F H`; `K` | erro | `t_v_pos`, `t_v_dc`, `t_i_pos`, `t2 t_k` |
| `.param` global ou do corpo | vale; quem o usa num dispositivo/modelo do mesmo nível também; uma cadeia (`q={p*2}`) também | `t_glob(_unused)`, `t_sub_local*`, `t2 t_chain*`, `t_tparam_model` |
| … usado noutro nível (global dentro de subcircuito, corpo num subcircuito chamado) | erro | `t2 t_tparam_in_sub`, `t_sub_local_nested_use` |
| … usado em `V`, linha `X`, cartão de controle | erro | `t2 t_tparam_v`, `t_tparam_x_actual`, `t_tparam_tran` |
| padrão da linha `.subckt` | vale (num `V` do corpo, erro) | `t_sub_default*`, `t2 t_sub_default_expr`, `t_sub_default_v` |
| atual da linha `X` | erro | `t_sub_actual` |
| corpo de `.func` chamado de dispositivo | vale | `t_func`, `t_func_kw` |
| … chamado de um `.param` | erro (o `.param` não nomeia `temper`, o `numparam` o avalia) | `t_func_glob` |
| cartões de controle (`.tran`, `.options temp=`), condição de `.if` | erro | `t_tran_card`, `t_options_temp`, `t_if_temper` |
| fonte `B` (`v=temper`) | variável da própria fonte B | `t_bsrc*` |
| `.param temper=99` | ignorado calado; `temper` continua a temperatura | `t_user_param*` |

Temperatura: a do circuito — último `.temp` em vigor (respeita `.if`),
senão o último `.options temp` (em qualquer posição, também dentro de
ramo não tomado), senão 27 °C (`t_opt50_*`, `t_temp75_*`,
`t_temp_and_opt`, `t2`); o `temp=` de uma instância **não** muda o
`temper` (`t_inst_temp`); `tnom` não entra (`t_model_tnom`). Numa
varredura `.dc temp` o ngspice reavalia (`t2 t_dc_sweep_model`: 1 mΩ,
50, 100 Ω).

Como ficou: o escopo global é resolvido a 27 °C; uma pré-passada
silenciosa pelo topo acha a temperatura do circuito (`.if` avaliados,
`.temp`/`.options` lidos pelo `dot.rs`); se ela não é 27 e algum
`.param` global depende de `temper`, o escopo é resolvido de novo. Cada
avaliação sabe onde está (`TemperUse`: modelo/dispositivo, `.param`,
proibido com o motivo), um nome que depende de `temper` fica marcado no
seu escopo e só é lido daquele escopo; a leitura vinda de corpo de
`.func` dentro de um `.param` é recusada como no ngspice. Erros sempre
com span e dizendo por quê.

**Varredura de temperatura.** O parser roda uma vez, antes do motor; o
`Circuit` não guarda o texto do deck para reexpandir, então não há
como reavaliar `temper` por ponto sem API nova. Um `.dc temp` num deck
que **usa** `temper` (num dispositivo ou modelo; um `.param` que
ninguém usa não conta) é erro com o span do `.dc`, e todo deck que usa
`temper` recebe um aviso de que ele foi avaliado uma vez, a T °C — vale
também para uma troca de `temp` depois do parse (as opções de motor do
wasm/R/Python). Uma solução completa precisaria de uma API que
reexpanda o netlist por temperatura: por exemplo
`dune_netlist::parse_*_at(…, temperature)` (ou uma opção no
`parse_with_loader`) e, no `dune-core`, um laço de `.dc temp` que, em
vez de só trocar `CKTtemp`, reconstrua o `Circuit` a partir do texto
guardado (o `Session` já reconstrói circuitos; faltaria a temperatura
como entrada do parser). Não feito: é mudança de API pública.

Diferenças (linhas no `KNOWN_DIFFS.md`): `.param temper=…` é erro (o
ngspice o ignora); com padrão `t={temper}` na linha `.subckt` e
`t=5` na linha `X`, o ngspice-47 usa **27** (o padrão vence a atual —
provável efeito da reescrita, `t2 t_sub_default_override`), o Dune usa
5; valor posicional de área de `Q`/`J`/`D` com `temper` vale no Dune (o
ngspice recusa a reescrita no `Q`, `t2 t_q_area`); fonte `B` com
`temper` segue sem suporte (a linguagem da fonte B é a do 3f5); `r=`
como sinônimo de `resistance=` (o ngspice aceita, `t1 t_inst_kw`) não
existe no Dune — à parte do `temper`.

### Funções do usuário (`.func`, `func.rs`)

Fase 8 (2026-11-14). No ngspice-47 o `.func` é uma **expansão textual
de macro** feita antes do avaliador de parâmetros, em
`frontend/inpcom.c` (Modified BSD; lido: `inp_get_func_from_line`,
`inp_grab_func`, `inp_expand_macro_in_str`, `inp_expand_macros_in_deck`
e a reescrita `.param f(x) = …` → `.func` em
`inp_fix_macro_param_func_paren_io`); o `numparam` não foi aberto. O
Dune **avalia** a chamada em vez de reescrever o texto
(`pexpr::Expr::eval_in`): o argumento chega como `f64`, o corpo é
compilado uma vez, e as regras observáveis da expansão foram sondadas
(`.claude/scratch/g9-probes/f1.txt`, `f2.txt`; testes em
`tests/func.rs`, deck `regress/func_op.cir` e `func_dc.cir`):

| Regra | ngspice-47 | Sonda |
|---|---|---|
| Formas | `.func f(x) {corpo}`, `.func f(x) = {corpo}`, `= corpo`, `corpo` nu (o resto da linha, com brancos), `'corpo'`; `f (x)` com branco; continuação `+`; `.param f(x) = corpo` é o mesmo `.func` (o primeiro token depois de `.param` tem `(`) | `f1 f_brace`, `f_eq`, `f_eq_bare`, `f_bare`, `f_quote`, `f_space_name`, `f_body_two_tokens`, `f_continuation`, `f_params_kw` |
| Aridade | zero ou mais formais; contagem errada na chamada é erro fatal; `.func f {2}` (sem parênteses), `f(x,)`, `f(x {…}` são erro fatal | `f_zero`, `f_arity_*`, `f_noargs_def`, `f_comma_empty`, `f_unbalanced` |
| Onde chama | `{…}`, `'…'`, valor nu depois de `=`, `.param` (com ou sem chaves), `.model`, linha `.subckt` e linha `X`, corpo de outro `.func` (em qualquer ordem), argumentos aninhados; `k` sem parênteses é parâmetro, não chamada | `f_in_*`, `f_kw_bare`, `f_in_func(_fwd)`, `f_nested_call`, `f_zero_noparen_use` |
| Ordem | as funções de um nível são colhidas antes de expandir: chamada antes da definição vale | `f_after_use`, `f2 f_local_redef_after` |
| Redefinição | a última vence (inclusive com outra aridade) | `f_redef`, `f_redef_arity(_old)` |
| Sombra | formal sombreia `.param`; `.func` sombreia função embutida (`sqrt`, `min` com 3 argumentos); `.param f` e `.func f` convivem | `f_shadow_param`, `f_shadow_builtin(_arity)`, `f_name_is_param` |
| Nomes do corpo | todo nome que não é formal é lido **onde a chamada está** (o texto vai para lá): `.param` global definido depois, parâmetro da instância que chama | `f_glob_param(_after)`, `f_param_uses_func_fwd`, `f_local_sees_inst_param`, `f2 f_glob_param_shadow_inst` |
| Escopo | léxico por nível de `.subckt`: a função de um corpo vale nele e nos subcircuitos definidos dentro dele, não fora nem num subcircuito que ele só instancia; a interna sombreia a externa | `f_local*`, `f2 f_top_calls_top_shadowed` |
| Chamada dentro de um corpo | resolve onde o corpo foi definido; se não achar, onde o texto foi parar (uma função global pode chamar uma que só existe dentro do subcircuito que a chama) | `f2 f_top_calls_local` |
| Preguiça | corpo nunca chamado não é avaliado (nome indefinido, sintaxe ruim passam); subcircuito não instanciado idem | `f_undef_in_body_unused`, `f_syntax_unused`, `f_local_unused_sub` |
| Recursão | *segfault* (rc −11), mesmo sem uso | `f_recursive`, `f_mutual`, `f_recursive_unused` |
| `temper`, aleatórias | `temper` no corpo vale a temperatura num valor de elemento; `agauss` no corpo sorteia | `f_temper`, `f_random`, `f2 f_random_func_note` |
| Fonte `B` | a expansão alcança a linha `B` (`v=f(3)`, `v=f(v(2))`) | `f_bsrc*` |

No Dune, além disso:

- **Funções aleatórias** no corpo valem o nominal, como fora dele
  (decisão G8). O ngspice transforma internamente um `.param` com
  `agauss` em `.func` para que cada uso sorteie de novo
  (`inp_fix_agauss_in_param`); como o Dune avalia no nominal, essa
  distinção não existe aqui.
- **Erros dentro do corpo** apontam para o corpo (span no `.func`) e
  dizem `(in function `f`)`; aridade e função desconhecida apontam para
  a chamada.
- **Recursão** (direta ou mútua, no mesmo nível) é erro com o span da
  chamada que fecha o ciclo, assim que o nível termina de ser lido — no
  topo sempre, num subcircuito só se ele for instanciado. Um ciclo que
  só aparece pelo recurso da chamada no lugar da expansão (global →
  local → global) para no limite de profundidade.
- **Limites** (`pexpr.rs`): ≤ 256 chamadas aninhadas por avaliação;
  orçamento de 2²⁴ passos (operações + nomes resolvidos dentro de
  corpos, e o cálculo de dependências pelos corpos) por deck — uma
  cadeia em que cada função chama a anterior duas vezes (2⁶⁰ chamadas)
  para com erro em ~0,5 s; ≤ 1000 formais. O avaliador continua sem
  recursão (pilha de quadros no heap).
- **Dependências:** um `.param` que chama uma função depende dos nomes
  que o corpo lê (transitivamente), para a ordem de avaliação e para a
  detecção de ciclo; um `.model` local que chama uma função do usuário
  não é compartilhado entre instâncias.

Diferenças (linhas no `KNOWN_DIFFS.md`): formal repetido (`f(x,x)`, o
ngspice usa o primeiro) é erro; corpo vazio (`.func f(x)`, aceito
calado) é erro quando chamado; recursão é erro, não *segfault*; o corpo
é uma expressão por si — `{3*-x}` é recusado como em qualquer
expressão (o ngspice substitui o texto e lê `3*-2`); `.func` em fonte
`B` é erro "not supported yet" (a linguagem da fonte B é a do 3f5).

### Condicionais (`.if`/`.elseif`/`.else`/`.endif`, `cond.rs`)

G9 de `docs/pdk-parser.md` (2026-11-14): o IHP SG13G2 escolhe a linha
do dispositivo dentro do `.subckt` por parâmetro da instância
(`rfmode`, `as`, `ng` par/ímpar). No ngspice-47 quem avalia é o
`numparam` (LGPL, não aberto); do `inpcom.c` só se leu que a linha
`.if(expr)` vira `.if{expr}` antes de chegar lá. Regras por sondas
(`.claude/scratch/g9-probes/i1.txt`–`i5.txt`; testes em
`tests/cond.rs`; decks `regress/if_subckt_op.cir` e `if_dc.cir`):

| Regra | ngspice-47 | Sonda |
|---|---|---|
| Condição | expressão de parâmetros **entre parênteses**, verdadeira se ≠ 0; `.if(a==1)` colado, brancos dentro, `=` sozinho compara, `&&`/`||`/`?:`, funções e `.func`; continuação `+` | `i1 i_top_*`, `i_glued`, `i_spaces`, `i_single_eq`, `i_value*`, `i_logic*`, `i_func_cond`, `i_udf_cond`, `i_continuation`, `i2 i_cond_ternary`, `i_cond_nested_parens` |
| Sem parênteses | `.if a==1`, `.if {a==1}`, `.if 'a==1'`, `.if` e `.elseif a==2` são fatais ("Bad syntax"); `.if ()` e `.if (1+)` também | `i_noparen`, `i_brace`, `i_quote`, `i_no_cond`, `i_empty_cond`, `i_bad_cond`, `i2 i_elseif_noparen` |
| O que lê | no topo, os `.param` globais (inclusive definidos depois); no `.subckt`, o escopo da instância (linha `X`, padrões, `.param` do corpo, escopo de quem chama, `m`) — cada instância escolhe o seu ramo; no topo não vê parâmetro de subcircuito | `i_fwd_param`, `i_sub_param2`, `i_sub_body_param`, `i2 i_sub_dyn`, `i_cond_m`, `i_sub_param_cond_top`, `i_sub_cond_fwd_body` |
| Cadeias e aninhamento | `.elseif` em qualquer número, `.else`, aninhamento (21 níveis testados) | `i_elseif*`, `i_nested*`, `i2 i_deep` |
| `.else if (c)` | é um `.else` **comum**: o texto depois de `.else` é ignorado, a condição nunca é lida | `i5 i_else_if_false_cond`, `i_else_if_then_else` |
| Avaliação | **toda** condição de um escopo que existe é avaliada — também no ramo não tomado e depois do tomado (nome indefinido lá é fatal) | `i_undef_untaken_elseif`, `i2 i_nested_untaken_undef`, `i_taken_then_elseif_bad` |
| Condicional | elementos, linhas `X`, `.model` (de topo e local, por instância), `.temp`, cartões de análise | `i_top_*`, `i2 i_x_top_false`, `i3 i_model_top_a/b`, `i1 i_sub_model`, `i4 i_d_temp_*`, `i3 i_op_card_false` |
| **Não** condicional | `.param` (topo e corpo: o último vence, dentro ou fora de ramo), `.func`, `.include`/`.lib` (lidos antes), `.options` (o último vence), definições `.subckt` (as duas existem; a primeira vence) e o corpo de um `.subckt` escrito dentro de um ramo | `i_param_in_branch(_false)`, `i_sub_param_in_branch`, `i_func_in`, `i_include_in`, `i4 i_d_options_*`, `i2 i_subckt_def_false`, `i_sub_nested_subckt_false` |
| Onde | topo, corpo de `.subckt`, seção de `.lib`; um bloco não atravessa `.ends` | `i2 i_lib_if`, `i1 i_sub_cross` |
| Preguiça | subcircuito não instanciado: condições não lidas | `i_sub_undef_unused` |
| Desbalanceado | `.endif`/`.else` soltos, `.else` repetido, `.elseif` depois de `.else` são erro; `.if` sem `.endif` é fechado calado no fim do deck ou do `.subckt` | `i_endif_alone`, `i_else_alone`, `i_else_twice`, `i_elseif_after_else`, `i_unclosed`, `i_sub_unclosed` |
| Texto sobrando | ignorado depois de `.endif`, `.else` e da condição | `i_trailing_endif`, `i_trailing_else`, `i_cond_trailing` |

Como ficou: a estrutura de blocos de cada escopo é lida uma vez (lista
plana de ramos em ordem de cartão, pais antes dos filhos, pilha de
blocos abertos — nada recursivo); cada cartão condicional guarda o ramo
mais interno que o contém. No topo, as condições são avaliadas ao ler
(os globais já estão resolvidos) e os cartões de ramo não tomado são
pulados, menos `.subckt`/`.ends`, `.options`, `.control` e `.end`. No
`.subckt`, `subckt::expand` avalia as condições por instância
(`Conds::evaluate`, uma unidade do orçamento da expansão por ramo) e
filtra elementos, `X` e `.model` locais; a busca do modelo local pega a
**última definição ativa** do nome (e só bins ativos). O IR público não
mudou.

Diferenças (linhas no `KNOWN_DIFFS.md`), todas em entradas que o
ngspice aceita calado ou lê errado: `.if` sem `.endif`, texto depois de
`.endif`/`.else`/da condição e `.else if` são erro com span (a mensagem
do `.else if` diz que é um `.else`); condição não finita (`(1/0)`, que
o ngspice trata como falsa) é erro; num ramo não tomado o Dune não olha
os cartões — o ngspice ainda substitui os `{…}` deles e um nome
indefinido ali é fatal (`i3 i_sub_param_body_after`,
`i_top_untaken_bad_elem`; lixo sem chaves passa nos dois,
`i_top_untaken_garbage`). Limites: ≤ 256 blocos aninhados por escopo
(erro com span; 10 000 aninhados dão um erro, em tempo linear).

### Entrada hostil e medidas

Limites: expressão ≤ 64 KiB (o IHP chega a 613 bytes; o limite antigo
de 512 caiu), aninhamento ≤ 100 (parênteses, chamadas, sinais, `!`,
`?:`); a avaliação é um laço sobre um programa de pilha (sem recursão),
e somas/potências planas de 30 000 termos avaliam; ≤ 1 000 000 de
`.param` globais por deck (o sky130 inteiro tem 170 406); a resolução
guarda só listas de dependências **distintas** (CSR) e recompila cada
definição na hora de avaliar, então a memória segue o número de
parâmetros, não de ocorrências de nomes; `Card::span_of` passou a busca
binária (cartão de 1,26 MB do `montecarlo.spice`). Medido em
`--release` (`.claude/scratch/g2-probes/big/`, `measure.py`, máquina com
carga ~25):

| Entrada | Antes (G1) | Depois |
|---|---|---|
| sintético, 170 000 `.param` com referências adiante, `agauss`, aspas (7,3 MB) | 1,5 s, 114 MB, 373 986 erros | 0,65 s, 92 MB, limpo |
| inversor sky130 achatado (7,1 MB, 14 060 `.param`) | 0,13 s, 94 MB | 0,15 s, 94 MB |
| arquivos de parâmetros do sky130 (44 arquivos, ~66 000 atribuições) | 2,5 s, 905 MB | 2,0–3,3 s, 840–885 MB |
| todos os 821 arquivos do sky130 por `.include` (para no orçamento de 256 MiB percorridos) | 3,3 s, 2,0 GB | 4,3 s, 1,6–1,8 GB |
| patológico: soma de 80 KB, 5000 `(`/`!`/`?:`/`abs(`, cadeia de 20 000, leque 3000×500, ciclo de 10 000 (4 MB) | 0,08 s, 26 MB | 0,2–0,4 s, 33 MB, 8 erros |
| 1 050 000 definições (11 MB) | 0,4 s, 198 MB | 1,4 s, 351 MB, 1 erro (teto) |

A memória dos casos grandes é do leitor (cartões ~6× o texto), não da
resolução.

## Parâmetros de subcircuito, binning e `m` (`subckt.rs`, `param.rs`)

G3, G4 e G6 de `docs/pdk-parser.md` (2026-11-14). Mesma regra de
proveniência da G2: contrato = comportamento observado do `ngspice` 47
em sondas mais o manual; `src/frontend/numparam/` (LGPL) **não** foi
aberto. Do código BSD foram lidos `frontend/inpcom.c`
(`inp_fix_subckt_multiplier`), `frontend/subckt.c` (tradução de nomes
de modelo, `model_name_match`), `spicelib/parser/inpgmod.c`
(`INPgetModBin`, `in_range`), `inp2m.c`, `inpdomod.c` e os `*par.c` dos
dispositivos (onde `scale` entra). As sondas — ~150 decks com a saída
do ngspice — estão em `.claude/scratch/g3-probes/` (`probe3.py CASOS
DIR` gera e roda; `dunerun.py DIR` roda os mesmos decks no Dune e
compara; `sN.txt` são os casos, `sN/` decks e logs); `s1 dyn_scope` =
caso `dyn_scope` de `s1.txt`. Cada teste de
`tests/subckt_params.rs` cita a sua.

### Como ficou

O IR público continua pós-avaliação e não mudou. No passo B o corpo de
um `.subckt` é lido só **estruturalmente**: os padrões da linha
`.subckt` e os `.param` do corpo viram `ParamDef` (as "formais", na
ordem), os cartões de elemento ficam como `&Card`, as linhas `X` como
`XCall` (nós, nome, atuais **não avaliadas**), os `.model` como
`ModelCard` (cartão + chaves de binning lidas do texto). Erros de
cartão do corpo ficam guardados e só são relatados se o subcircuito
for instanciado. O `subckt::expand` avalia por instância:

1. resolve o escopo da instância com `param::resolve_local`: formais e
   depois as atuais da linha `X` (que portanto vencem), pelo mesmo
   algoritmo do escopo global (último vence, referência adiante, ciclo
   = erro), lendo os nomes que faltam no escopo **de quem chama**;
2. parseia cada cartão de elemento do corpo nesse escopo
   (`parse_element`), renomeia, resolve o modelo (com binning) e aplica
   o multiplicador;
3. um `.model` local só é avaliado quando um elemento da instância (ou
   de uma instância que ela chama) o usa — uma cópia por instância,
   `x1.nome`; instâncias que concordam nos valores de todos os nomes
   que o cartão lê compartilham a mesma cópia (com o nome da
   primeira).

`.model` de topo: avaliado uma vez com os globais, diagnósticos
guardados; só são relatados se algum elemento o usa, e um modelo de
topo não usado que falhou some do netlist (um válido continua lá).

### Regras observadas

| Regra | ngspice-47 | Sonda |
|---|---|---|
| Precedência | linha `X` > padrão da linha `.subckt` = `.param` do corpo = `params:` > escopo de fora | P04, P23, `s1 undeclared_x_bodyparam` |
| Ordem dentro do corpo | todas as formais formam um escopo: o **último** `.param`/padrão de um nome vence (o `.param w=2` do corpo vence o `w=1` da linha), referência adiante vale | `s1 body_redef`, `line_then_body`, `body_fwd`; `s2 body_param_after_elem_ref_glob` |
| Atuais | avaliadas **no escopo da instância chamada**: `w={l}` lê o `l` do chamado, não o de quem chama; uma atual pode ler outra atual ou um `.param` do corpo | `s3 xcl_both`, `xcl_both_mid`, `xcl_actual_actual`, `xcl_actual_body` |
| Autorreferência | uma definição que nomeia a si mesma (`.param w={w*2}`, `w={w*3}` na linha `X`) lê o valor do escopo de fora; sem valor lá, "unknown parameter"; `v={w}` + `w={v}` é ciclo | `s1 body_self`, `s3 x_self_nested`, `line_body_self`, `body_self_chain` |
| Escopo dinâmico | nome que a instância não define é procurado na instância **que a chama**, depois nas de cima, depois no global — um subcircuito definido no topo vê os parâmetros de quem o instancia | `s1 dyn_scope`, `dyn_scope_body`, `lex_nested`; `s2 dyn_chain`, `dyn_default` |
| Atual sem formal | ignorada calada se o subcircuito tem alguma formal (e não fica visível no corpo); **erro** "Mismatch: 0 formal but 1 actual params" se não tem nenhuma | `s2 count_more`, `s4 m_undecl_visible`, `s1 undeclared_x_noparams`, `x_glob_override` |
| Maiúsculas, `params:`, brancos | nomes sem diferenciar maiúsculas; `params:` opcional nas duas linhas; `w = 2` vale | `s1 case_*`, `params_kw_x`, `port_eq_space`, `x_eq_space` |
| Preguiça | subcircuito nunca instanciado: nada no corpo é avaliado nem parseado (lixo, ciclo, nome indefinido passam); instanciado: todo `.param` dele conta, usado ou não; `.model` não usado (local ou de topo) não é avaliado; usado com nome indefinido é erro | P05f, `s1 body_undef_unused`, `model_undef_*`; `s2 sub_unused_*`, `body_cycle*` |
| Não finito | atual `{1/0}` só é erro onde chega a um cartão | `s2 x_value_nonfinite(_used)` |
| Modelo por instância | `.model` local dependente de parâmetro da instância: uma cópia por instância | P08, `s1 model_local_inst` |
| "Dado" | o valor chega ao cartão como texto: `ad={ad}` com `ad=0` continua escrito, logo dado | P23 |

Diferenças: atual sem formal ignorada **com aviso**; `params: w` sem
valor é erro "has no value" quando a instância é avaliada (o ngspice
também falha, "Too few parameters", mesmo com `w=` na linha `X`).

### Binning (G4)

Feito na expansão, no IR, para qualquer nível: o BSIM4 (em porte
paralelo) herda sem código novo. Regra (`INPgetModBin`, sondas P09 e
`s5`): se nenhum modelo tem o nome exato, `nome` casa a família
`nome.<dígitos>` (`nome.01` vale, `nome.a` não); só bina `M` com
`nmos`/`pmos` de nível BSIM3 (8, 49), BSIM4 (14, 54) ou HiSIM (68, 73)
e com os quatro limites `lmin lmax wmin wmax`; o elemento precisa ter
`l` e `w`; `L·scale` e `W·scale` (`W/nf` com `wnflag` — da instância,
ou `.option wnflag`; padrão 0, como o ngspice sem modo `hs`/`spe`)
devem caber nos dois intervalos, **inclusivos** com tolerância
**absoluta** de 1e-9; os limites não são escalados; o bin **definido
por último** é tentado primeiro; nenhum bin → erro com o span do
elemento ("could not find a valid model for …"). Uma família local ao
subcircuito vence o modelo exato e a família globais (`s5 b_local_*`,
o `subckt.c` renomeia a referência para `x1:nome`); o limite pode ser
expressão (avaliada no escopo da instância; um limite que não avalia
pula o bin, como o `parse_line`). O elemento passa a nomear o bin
escolhido (`nch.2`, `x1.nch.71`) — o elaborador não sabe de binning.
Os coeficientes `l*/w*/p*` do BSIM3 já eram aplicados pelo porte
(`bsim3/temp.rs`, com `binunit`).

### `m` na linha `X` (G6) e o `m` do próprio dispositivo

Semântica do `inp_fix_subckt_multiplier` (modo normal, sem `hs`;
sondas P13 e `s4`): todo subcircuito chamado com `m=` em **alguma**
linha `X` do deck, e que não declara `m` (na linha nem num `.param` do
corpo), ganha a formal `m=1`, e cada linha de dispositivo e `X` do
corpo ganha `m={m}` no fim — o `m` próprio do dispositivo é
**substituído**, não multiplicado (`m_r_own`: `r … m=2` com `x … m=3`
dá R/3; `m_r_own_otherinst`: outra instância sem `m` leva m=1 e perde
o `m=2`); um `X` aninhado recebe o `m` de quem chama, substituindo o
seu (`m_nested_both`). `m` é visível nas expressões do corpo. Com
`m` declarado é parâmetro comum, sem multiplicar. As letras `V E H A K
N O P S T U W Y` e a fonte `B` de tensão ficam como estão.

O `m=` do **próprio cartão** — no nível de topo ou num subcircuito que
nenhuma linha `X` multiplica — age pelo mesmo caminho
(`subckt.rs::apply_multiplier`; sondas `m*.cases` em
`.claude/scratch/fix-probes/`); o último `m=` do cartão vence
(`m_last`). Por tipo no Dune:

| Dispositivo | ngspice-47 | Como o `m` chega no Dune |
|---|---|---|
| BSIM3/BSIM4/VDMOS (`M`), VBIC (`Q` nível 4/9) | nativo | parâmetro `m` nativo |
| `R` (valor, `resistance=` ou geometria de modelo) | `G = m/R` | parâmetro `m`, lido pelo elaborador (`ResistorI`, como o `RESm`) |
| `C` (valor ou geometria) | `C·m` | parâmetro `m` (`CapacitorI`) |
| `L` | `L/m`; o `K` lê o `L` sem dividir (`muttemp.c`) | parâmetro `m` (`InductorI`; sonda `l_m_k`) |
| `D` | correntes, condutâncias e cargas ·m; joelho da ruptura casado como `IS·AREA·M` contra `M·IBV`; 1/f com `|I/m|` | parâmetro `m` (`DiodeTemp`; o `area`·m de antes errava a ruptura em `vt·ln m` — 18 mV em `d_m_bv`) |
| `J`, `Z`, `Q` Gummel–Poon | estampas ·m, inclusive o `gmin` da junção; ruído ·m (1/f `m·KF·|I|^AF`); `.disto` com `area·m` | parâmetro `m` (`BjtI`/`JfetI`/`MesI` multiplicam as estampas; a área fica como está) |
| `G`, `F` | ganho·m | ganho·m |
| `I` DC/AC | valor·m | valor·m |
| `I` com `PULSE`/`SIN`/`EXP`/`PWL`/`SFFM` | `m·valor(t)` | níveis da forma ·m (`V1 V2`, `VO VA`, valores do `PWL`; sonda `s1/isrc_m`) |
| `I` com `DISTOF1/2` | magnitude **sem** m (`CKTdisto` lê `ISRCdF1mag`; sonda `s1/isrc_d`) | idem (literal) |
| `B` de corrente | valor·m | `m·(expr)` |
| `B` de tensão | valor·m (só o `m` próprio; a linha `X` não chega nele) | `m·(expr)` (sonda `bv_m`, `bv_sub_own`) |
| `X` aninhado | atual `m` | atual `m` |
| `V`, `E`, `H` | "unknown parameter (m)" | erro com span (o `E`/`H` aceitava qualquer palavra-chave calado) |
| MOS 1–3/6, BSIM1/2; `I` com `TRNOISE`/`TRRANDOM` | multiplica | erro "not supported yet" com o span (salvo m=1) |

Diferenças: m ≤ 0 é erro (o ngspice simula: m=0 abre, m<0 inverte;
sondas `r_m_zero`, `r_m_neg`); o marcador é fechado sobre o grafo de
chamadas em qualquer ordem — o ngspice marca numa passada em ordem de
deck e um subcircuito aninhado definido **antes** de quem o chama falha
com "Mismatch" (`m_nested_def_before`); subcircuitos aninhados
(definidos dentro de outro) seguem a própria marcação — o ngspice só
acrescenta `m={m}` até o primeiro `.ends`. Desde 2026-11-14 o `gmin`
de `J`/`Z`/`Q` escala com o `m` como no ngspice (`q_m_gmin`, junção
cortada com `IS=1e-20`: −0,25 V nos dois; antes o `area`·m dava −0,5 V
no Dune; sondas `s1/qjz.cases` em `.claude/scratch/dev-probes`).

Por que MOS 1–3/6 e BSIM1/2 seguem recusando (lido no C do ngspice-47):
no MOS1/2/3 o `m` não é multiplicador de estampa — entra na física
(`DrainSatCur = m·tSatCur`, `Beta·m`, sobreposições `·m·W`/`·m·L`,
áreas de junção ·m, e o 1/f com `|cd/m|`), em ~12 pontos por modelo;
no MOS6 só o `mos6load.c` multiplica as estampas (o `mos6acld.c` e o
`mos6pzld.c` ignoram o `m` — inconsistente); no BSIM1/2 load, AC, pz e
ruído multiplicam estampa a estampa (limpo), mas o `.disto` do BSIM1
(`b1dset.c`) escala a geometria — porte possível sem API pública,
deixado para depois. `TRNOISE`/`TRRANDOM` com m ≠ 1: o sorteio
poisson do `TRRANDOM` não é linear nos argumentos e o ruído
transiente do Dune não reproduz a sequência do ngspice para conferir.

`area=` como palavra-chave vale no `D` (antes "unknown parameter") e
no `Q` Gummel–Poon (antes ignorado calado); o valor posicional vence o
`area=` em `D`, `Q`, `J` e `Z`, como no `INP2D`/`INP2Q`/`INP2J` (que
o fixam depois das palavras-chave; sondas `q_pos_kw`, `j_pos_kw`) — o
`J`/`Z` faziam o contrário. O ngspice-47 recusa o `area` posicional no
`D` ("could not find a valid modelname", `d_area2_pos`); o Dune segue o
3f5.

### `.option scale` (G5)

Aplicado na expansão, depois do binning (que escala L e W sozinho),
onde o ngspice-47 o aplica (os `*par.c`; sondas `s6`): `l`, `w` de
MOSFET de nível 1, 2, 3, BSIM3 e BSIM4 × `scale`, `ad`, `as` ×
`scale²`, `pd`, `ps` × `scale`; `l`, `w` de `R` e `C` × `scale`. Não
são escalados: MOS6, BSIM1/2, VDMOS, áreas de diodo/BJT/JFET, valores
e qualquer parâmetro de modelo (`ld=0.1` continua 0,1 m,
`sc_model_ld`); `scalm` não faz nada no ngspice-47 para esses níveis.
O IR passa a carregar a geometria **já escalada** e o `scale` fica nas
opções só como informação; o `dune-core` não avisa mais "option scale
is not implemented". `scale` ≤ 0 ou não finito é erro com span. Deck
`regress/scale_geom.cir`.

### Entrada hostil e custo

- Profundidade ≤ 64; subcircuito que instancia a si mesmo (direto ou
  por outro) é erro na primeira repetição (o ngspice para após ~22
  níveis, H6).
- Orçamento único de 2²⁰ unidades: cada elemento emitido custa
  1 + bytes do nome e dos nós / 64; cada instanciação, 1 + número de
  definições do escopo; cada cartão parseado por instância,
  1 + bytes/256; cada modelo avaliado, 1 + bytes/256 (1 se
  compartilhado); cada seleção de bin, 1 + candidatos/64, mais 1 por
  candidato cujos limites são expressões.
- Ciclos entre escopos não existem por construção (o escopo de quem
  chama está resolvido antes do chamado); ciclo dentro de um escopo é
  o erro de sempre.
- Cada diagnóstico distinto (span + mensagem) da expansão é relatado
  uma vez — o mesmo erro em mil instâncias é uma linha —, até 1000.
- Os dois *timeouts* do libFuzzer em `Expander::emit` (13 s e > 10 s
  com os sanitizadores; 0,25 s e 0,8 s em release; entradas em
  `tests/fixtures/fuzz/emit-timeout-{1,2}.bin`) eram decks do corpus
  mutilados com subcircuitos recursivos em leque: a expansão ia até o
  teto de 2²⁰ — 190 000 e 660 000 elementos com nomes de até 64
  níveis, e um diagnóstico por caminho (665 000). Hoje: 0,9 ms e
  0,14 ms (recusa da recursão, cobrança por byte, diagnóstico único).

Medido em `--release` (`.claude/scratch/g3-probes/measure.py`, mínimo
de 3; os PDK param na elaboração por falta do BSIM4):

| Entrada | Antes | Depois |
|---|---|---|
| inversor sky130 por `.lib` (2 instâncias × 180/108 bins) | 0,13 s, 86 MB, 5080 erros | 0,10 s, 59 MB, limpo |
| inversor gf180 | 0,01 s, 14 MB, 154 erros | 0,01 s, 14 MB, limpo |
| 1000 nfet sky130 de mesma geometria | (para no parse) | 0,12 s, 62 MB |
| 1000 nfet sky130 de geometrias distintas | (para no parse) | 0,19 s, 97 MB |
| 20 000 nfet sky130, mesma / distintas | — | 1,1 s, 138 MB / 1,5 s, 526 MB |
| sintético: 2000 instâncias, 200 bins de 300 parâmetros-expressão | (para no parse) | 0,18 s, 33 MB (1,3 s, 96 MB sem o compartilhamento) |
| `dune-oracle --ci` inteiro | 82,67 G instruções | 82,56 G instruções |

Reparsear o texto de um cartão de modelo por instância custa ~50 µs/KB;
com o compartilhamento só as combinações distintas pagam, e não foi
preciso compilar as expressões do corpo uma vez só.

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
gt0 lt0 ge0 le0`; os parâmetros de instância `tc1 tc2 temp dtemp
reciproctc reciprocm` (o `m` entra: `m·(expr)` nos dois tipos, como o
`asrcload.c`); `{}`/`.param` dentro da expressão.

Erros: toda expressão malformada vira diagnóstico com o span do
trecho (`syntax error`, `no such function`, `badly formed node
voltage`/`branch current` — as mensagens do C —, nome solto,
vírgula fora de `v()`, limites). Fuzz: o alvo `parse_deck` também
alimenta `parse_expr` e um cartão B com os bytes; o smoke
determinístico (`tests/fuzz_smoke.rs`) muta os cartões B das sondas
`bsrc_*` e leva o parser aos limites.

`dune_netlist::parse_expr(texto)` expõe o mesmo parser (expressão
inteira; texto sobrando é erro).

## R, C e L comportamentais (`behav.rs` ← `inp_compat` do ngspice-47)

O ngspice-47 reescreve no front end (`src/frontend/inpcom.c`,
`inp_compat`, Modified BSD) um R, C ou L cujo valor lê o circuito.
Critério (`b_transformation_wanted`): o texto depois do nome e dos dois
nós contém `v(`, `i(` ou as palavras `time`, `temper`, `hertz` (não
precedidas de letra/dígito/`_`), ou, num capacitor, `q=`. A expressão é
o que vem entre o primeiro `{` (ou `'`, que o ngspice converte em `{`)
e o fecho; sem delimitador o ngspice para com "{...} or '...' around
equation's right hand side are missing!" — no Dune, erro com o span. A
forma posicional (`r1 a b '…'`) e a de palavra-chave (`r=`, `c=`,
`l=`) são a mesma coisa para ele. O Dune faz a mesma reescrita, com a
mesma topologia e os mesmos nomes (vetores e nós internos iguais aos do
rawfile do ngspice; dentro de subcircuito a expansão dá `b.x1.brx`,
`x1.c1_int1`…, como lá):

| cartão | gerado (ngspice e Dune) |
|---|---|
| `Rxxx n1 n2 r={eq}` | `bRxxx n1 n2 i = v(n1,n2)/(eq)`, com o `m` multiplicando a corrente (`reciprocm=0`) |
| `Cxxx n1 n2 c={eq}` | `gCxxx Cxxx_int1 0 n2 n1 1e9`, `rCxxx Cxxx_int1 0 1e-9`, `cCxxx Cxxx_int1 Cxxx_int2 1e-6`, `vcurrmeasCxxx Cxxx_int2 0 0`, `rCxxxpar n2 n1 1e15`, `bCxxx n1 n2 i = 1e6*i(vcurrmeasCxxx)*(eq)` |
| `Cxxx n1 n2 q={eq}` | `gCxxx n1 n2 Cxxx_int1 0 m`, `lCxxx Cxxx_int1 0 1`, `bCxxx 0 Cxxx_int1 i = (eq)` |
| `Lxxx n1 n2 l={eq}` | `fLxxx Lxxx_int2 0 bLxxx -1`, `lLxxx Lxxx_int2 0 1`, `bLxxx n2 n1 v = v(Lxxx_int2)*(eq)/m` |

Os cartões geram elementos direto (`elem.rs::behavioral_rlc`), não
texto; o `F` do L sente a fonte `B` de tensão, o que o núcleo passou a
aceitar (`circuit.rs::ctrl_branch`; o 3f5 só aceitava fonte `V`, o
`CKTfndBranch` do ngspice aceita qualquer ramo). Cada elemento extra
(até cinco) é cobrado do orçamento da expansão.

**A expressão.** O ngspice entrega a linha `B` ao numparam com cada nome
de parâmetro entre chaves (`inp_modify_exp`) e a analisa com a gramática
da fonte B dele (`inpptree-parser.y`): `^` e `**` associam **à
esquerda**, o menos unário fica **abaixo** de `^` (`-2^2 = -4`,
`2^3^2 = 64`; sondas `r_unaryneg`, `r_powassoc`). A gramática da fonte
`B` do Dune é a do 3f5 (`^` à direita; `KNOWN_DIFFS.md`), então a
expressão de um R/C/L comportamental **não** passa pelo parser do 3f5:
`behav.rs::translate` a analisa com as regras do ngspice, avalia os
parâmetros no escopo do cartão (como o numparam) e monta a árvore
`ir::Expr` com a estrutura do ngspice — ou seja, um `^` num R/C/L
comportamental vale o que vale no ngspice, ao contrário de um `^` escrito
numa fonte `B` do próprio deck. Funções:

- com par no 3f5 (mesmo valor no domínio comum): `abs acos acosh asin
  asinh atan atanh cos cosh exp sgn sin sinh sqrt tan tanh u uramp`;
  `ln` **e** `log` são o logaritmo natural no ngspice (`PTlog`) →
  `ln` do 3f5; `log10` → `log` do 3f5. Diferem fora do domínio: o
  ngspice dá `HUGE` para `ln`/`log`/`sqrt` de negativo (o 3f5 usa `|x|`)
  e satura `exp` acima de 227,96 em 1e99;
- reescritas exatas (valores iguais, salvo onde dito): `pow(a,b)` e
  `a^b`/`a**b` → `^` do 3f5 (igual para `a ≥ 0` e para expoente inteiro;
  com `a < 0` e expoente não inteiro o ngspice dá 0 e o 3f5 `|a|^b`;
  `pow(0,b<0)` dá 0 no ngspice); `pwr(a,b)` → `sgn(a)·|a|^b` (no
  `a = 0` o ngspice dá `0^b`, o Dune 0); `min(a,b)` →
  `a·u(b−a) + b·u(a−b)`, `max(a,b)` → `a·u(a−b) + b·u(b−a)` (valor
  exato, inclusive no empate; a derivada no empate exato é a média das
  duas, no ngspice a de um dos lados);
- recusadas com o span: `?:`, comparações e operadores lógicos;
  `ceil floor nint u2 pwl eq0 ne0 gt0 lt0 ge0 le0 ddt`; `time`,
  `temper`, `hertz` (a fonte `B` do Dune não tem tempo nem temperatura —
  G7); função desconhecida; parâmetro indefinido.

**Palavras-chave.** `m` como no ngspice (`eval_m`/`eval_mvalue`). No
capacitor por capacitância o ngspice escreve o ganho do `G` como
`"%se9"` com o `m` formatado em `"%15.8e"`: sem `m=` sai `1e9`, com
`m=2` sai `"2.00000000e+00e9"`, que lê **2** — o capacitor encolhe 1e9
vezes e some do circuito (sonda `c_ac_m`: |v| = 1000 Ω puro). Portado
literalmente (`regress/behav_ac`, `c2`). `tc1`/`tc2` diferentes de zero:
recusados (a fonte `B` do Dune não tem coeficientes de temperatura); zero
passa. `noisy=1`/`noise=1` (o subcircuito de ruído extra do ngspice):
recusado. Outras palavras-chave: aviso, e descartadas como o ngspice
descarta. Num subcircuito multiplicado por um `m` de linha `X`: recusado
(a ordem entre `inp_fix_subckt_multiplier` e `inp_compat` decide qual
`m` vence; não sondado).

**Entrada hostil.** Limites aplicados durante a construção da árvore,
nó a nó (cada subárvore leva profundidade e contagem): texto ≤
`pexpr::MAX_LEN` (64 KiB), parênteses/chamadas/sinais aninhados ≤ 100
(`MAX_NEST`, a recursão do parser), profundidade ≤ `EXPR_MAX_DEPTH` − 4
= 508 e nós ≤ `EXPR_MAX_NODES` − 8 = 9992 (a margem é do que
`elements` põe em volta); `min`/`max`/`pwr` checam antes de clonar.
Nenhuma árvore além dos limites é construída, percorrida ou destruída
(a cadeia à esquerda `a+a+…` crescia um nível por termo). Teste
`behav::tests::hostile_expressions_are_bounded` (cadeia de 1e6 termos,
1e5 parênteses, 1e4 chamadas, cadeia de `^`, numa thread de 2 MiB — a
recursão de 100 níveis passa de 1 MiB em debug); sementes de fuzz
`behav_*.cir` no corpus do `parse_deck` e os cartões R/C/L dos decks
`behav_*` no `fuzz_smoke`.

Decks `regress/behav_{op,dc,ac,tran}` (goldens do ngspice-47, margens
0,000); sondas `behav.cases`/`behav2.cases` em
`.claude/scratch/dev-probes/s3`.

**PDKs.** Os 24 usos do sky130 (`res_high_po_*`, `res_xhigh_po*`,
`cap_var_*`, `nfet_20v0*` e parentes) e os 4 do gf180
(`nmos/pmos_10p0_asym`, `nplus_u` …) continuam sem rodar, agora por
outros motivos: todos dependem de `.param` com `temper` (G7, recusado no
parser de parâmetros) e os resistores do sky130 trazem `tc1`/`tc2`
diferentes de zero (bancada `.claude/scratch/dev-probes/s3/
sky130_behav_dc.cir`: "`temper` in parameter expressions is not
supported yet", depois "tc1 on a behavioural R/C/L is not supported
yet"). As funções que eles usam (`min`, `max`, `pow`, `pwr`, `**`,
`abs`, `log`, `cosh`, `tanh`, `exp`, `sqrt`) já são aceitas.

## Valores por palavra-chave em R, C e L

O ngspice-47 aceita `r=` (sinônimo de `resistance=`), `c=`
(`capacitance=`) e `l=` (`inductance=`) com valor numérico; o
`INPdevParse` os aplica depois do valor posicional, então a palavra-chave
vence (`r1 a b 3k r=2k` dá 2 kΩ; sondas `kw.cases` em
`.claude/scratch/dev-probes/s3`). O Dune faz igual (antes: "resistor has
no value" para `r=`, `c=` e `l=`, e o valor posicional ficava). `tc1`/`tc2` de instância num R substituem os do
modelo (`restemp.c`); num C ou L, diferentes de zero, são recusados (o
núcleo não tem temperatura nesses dois; antes eram ignorados com nota).
Num C, `l=`/`w=` seguem sendo a geometria. Decks `regress/rlc_kw_{op,ac}`.

## Pendências conhecidas

- `.plot` ignora as faixas `(PLO, PHI)` (sem plotter ainda).
- Variáveis de saída com espaços internos (`v ( 1 )`) viram `Raw` com
  aviso — o frontend do 3f5 tinha um lexer próprio para isso.
- Fuzzing (`cargo fuzz`) entra junto com a estabilização do crate
  (princípio 4 do CLAUDE.md).
