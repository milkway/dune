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
   Diferenças: no `.model` o ngspice só avisa e segue com valor
   mutilado, e num subcircuito não instanciado ele nem avalia (o Dune
   avalia o corpo de todo subcircuito até a G3 do `docs/pdk-parser.md`).
   Parâmetros na linha `X` (`x1 a b inv w=1u`, `params:`) e `.param`
   dentro de `.subckt` ainda não existem (G3): erro preciso com span.
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
  (string) é erro. Um `.param` dentro de `.subckt` é erro "not
  supported yet" (G3) e não vaza para o escopo global (antes vazava).
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
nos testes. `temper` é lido como nome; sem um `.param temper` definido,
avaliá-lo é erro "not supported yet" (G7: precisa reexpandir quando a
temperatura muda, e no ngspice ele só vale em certas posições — erro em
`.param` global e em `v1 1 0 {temper}`, valor em `r={temper}`, P07/c4).

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

### G3: onde entra o escopo de subcircuito

O IR público continua pós-avaliação (`ParamValue::Num`): a G2 não mudou
tipo público nenhum, e a G3 não precisa mudar. O que a G3 deve guardar
**sem avaliar** é o corpo do subcircuito como **cartões** (`deck::Card`,
com spans) em vez de `Element`/`Model` já parseados: hoje o passo B
parseia o corpo com o ambiente global e o `subckt::expand` só renomeia.
Com a G3 o `SubcktDef` passa a carregar os cartões do corpo, os
padrões da linha `.subckt` e as definições `.param` do corpo (mesmo
`ParamDef`/`collect_params` de hoje), e a avaliação acontece **na
expansão**, por instância: um `ParamEnv` filho com as atribuições da
linha `X` sobre os padrões da linha `.subckt` e do corpo (S4: `X` >
padrões > global sombreado), resolvido pelo mesmo `param::resolve`
(referência adiante, último vence, ciclo) com consulta ao pai, e só
então cada cartão do corpo é parseado (`parse_element`/`parse_model`
com esse ambiente) — o que dá de graça a preguiça (S14: corpo de
subcircuito nunca instanciado nem é parseado) e o `.model` por
instância (S13). Para isso o `ParamEnv` precisa de um ponteiro para o
ambiente pai na consulta (`lookup` hoje olha um mapa só) e o orçamento
de 2²⁰ do `subckt.rs` precisa cobrar também os cartões parseados por
instância.

O custo a vigiar é o sky130: 288 cartões de modelo de ~300 parâmetros
por instância de MOS; reparsear o texto por instância é linear mas
pode pesar — se pesar, compilar cada expressão do corpo uma vez
(`pexpr::Expr` é `Clone` e independente do ambiente) e reavaliar por
instância. `temper` (G7) entra no mesmo ponto: é um nome do ambiente da
expansão, e uma varredura de temperatura reexpande.

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
