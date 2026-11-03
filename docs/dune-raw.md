# dune-raw — notas de implementação

**Referência:** `corpus/spice3f5/src/lib/fte/rawfile.c` (`raw_write`,
`raw_read`; Wayne A. Christopher, 1986), `types.c` (tabela de tipos),
`fteconst.h` (flags `VF_*`). Validado contra o ngspice-47
(`corpus/ORACLE`): fixtures reais em `crates/dune-raw/tests/data/`, e a
saída do `dune-raw` (ASCII e binária) recarregada no ngspice com os
mesmos valores (2026-11-03).

## O que foi portado fielmente

- Cabeçalho na ordem e grafia do `raw_write`: `Title`, `Date`,
  `Plotname`, `Flags` (`real|complex`, ` unpadded` quando aplicável),
  `No. Variables`, `No. Points`, `Dimensions` (só multi-dimensional),
  `Command`/`Option`, tabela `Variables:` com atributos
  `min=/max=/color=/grid=/plot=/dims=`.
- ASCII: 15 decimais (`DEFPREC`), expoente estilo C (`e+03`), índice do
  ponto iniciando a linha, um valor por linha, **linha em branco após
  cada ponto** (3f5; o ngspice não a escreve, o leitor aceita ambos).
- Plot complexo quando *qualquer* vetor é complexo; vetores reais em
  plot complexo ganham parte imaginária zero.
- `padded`/`unpadded` com comprimento por vetor via `dims=` (lógica do
  `fixdims`: produto > npoints → volta ao default; ≤ → trunca).
- Leitor flexível como o C: keywords case-insensitive, whitespace livre
  nos valores (o C usa `fscanf`), primeiro `Variables:` pode trazer a
  primeira variável na mesma linha, múltiplos plots por arquivo,
  renomeação `2` → `V(2)` para nomes iniciando em dígito
  (`ft_typabbrev`), `scale=` pendurado vira aviso.
- Linha não reconhecida e não vazia aborta a carga ("strange line"),
  como no C; avisos não fatais (flag desconhecida, atributo ruim,
  `Command:` antes do primeiro `Plotname:` — o ngspice escreve isso)
  ficam em `Rawfile::warnings`.

## O que foi reescrito e por quê

- **Tipo de variável guardado por nome** (`VarType`, com `Other(String)`
  para nomes fora da tabela). A tabela `types[]` do `types.c` e os
  `SV_*` do `fteconst.h` divergem a partir do índice 5 (a tabela manteve
  4 entradas de ruído que as constantes colapsaram em 2) — bug latente
  do 3f5 que desaparece quando o nome é a representação canônica.
- **Dados por coluna** (`Data::Real/Complex`, `Vec` por variável) em vez
  da lista ligada `dvec`; a escala é `variables[0]` por invariante (o
  escritor C a move para a frente antes de escrever).
- **Binário é little-endian fixo** (o C escreve double nativo; todos os
  alvos do Dune — x86-64, aarch64, wasm32 — são LE).
- Contagens hostis não pré-alocam além de 64 Ki entradas por coluna;
  truncamento e lixo produzem `Error::Parse { line, msg }`, nunca
  pânico (princípio 4; teste `hostile_input_never_panics`).

## Micro-divergências deliberadas do escritor

Nenhuma afeta a leitura pelo 3f5/ngspice (verificado com o ngspice-47):

1. Parte imaginária de valor zero sai como `0.000000000000000e+00`, não
   o literal `,0.0` que o 3f5 usa para *vetor real em plot complexo* —
   distinção que não sobrevive à leitura (todo vetor de plot complexo
   vira complexo) e que o próprio ngspice não preserva.
2. `plot=` na linha da variável imprime o valor de `grid` — fiel ao C,
   que tem esse bug (`rawfile.c:139`); documentado aqui para não parecer
   acidente.
3. `No. Points` sem os espaços de padding que o ngspice deixa ao
   retro-corrigir o valor (o 3f5 também não os deixa).

## Fora de escopo (por enquanto)

- Execução de `Command:` ao carregar (o C chama `cp_evloop`; para o Dune
  as linhas são dados inertes — nunca executaremos comandos de um
  arquivo de dados).
- `Dimensions` além de guardar/validar o produto (reshape fica para
  quando um `.dc` aninhado do corpus precisar).
