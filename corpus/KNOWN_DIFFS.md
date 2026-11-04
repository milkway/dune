# KNOWN_DIFFS — divergências conhecidas

Divergências deliberadas ou inevitáveis entre Dune, SPICE3f5 e ngspice.
Cada linha aponta a decisão (`DECISIONS.md`) ou a causa. Nunca silenciar
uma divergência por tolerância larga (D-0006).

| Área | 3f5 | ngspice | Dune | Causa / decisão |
|---|---|---|---|---|
| Cabeçalho do rawfile | `raw_read` rejeita `Command:` antes do primeiro `Plotname:` ("Error: misplaced Command: line", `fte/rawfile.c:376`) e descarta a linha | escreve `Command: ngspice-47, Build` entre `Date:` e `Plotname:` em todo rawfile | leitor espelha o 3f5: descarta a linha e registra o aviso `misplaced Command: line` (todo golden gerado pelo ngspice o carrega) | fidelidade ao leitor do 3f5 (D-0002); o aviso é esperado e inofensivo |
| Corpus: `mixdisto.cir` | rodado interativamente com `disto` no prompt (as linhas `*disto` do deck são comentários) | `-b` sem cartão de análise produz rawfile vazio | sem golden; listado em `corpus/SKIP` | deck upstream sem cartão de análise |
| Nomes na expansão de `.subckt` | `r:x1:r1`, nós `x1:n1` | `r.x1.r1`, nós `x1.n1` | segue o ngspice | o oráculo compara nomes de vetores com o ngspice (D-0006); ver `docs/dune-netlist.md` |
| R/C: token na posição do modelo | aceita qualquer token como modelo (`r1 1 2 100 w=5u` lê `w` como modelo e 5e-6 como resistência) | trata keyword como parâmetro | keyword conhecido é parâmetro, não modelo | bug do 3f5 que o ngspice corrigiu; `docs/dune-netlist.md` §4 |
| Comentários | linha inteira `*` (e `$`/`#` na coluna 1) | também `$`/`;` de fim de linha | como o ngspice | D-0002 (superconjunto pragmático) |
| Último ponto do `.dc` | comparação crua `valor > stop`: drift de acumulação pode comer o ponto final (ex.: `0:5m:0.25m` dá 20 pontos) | guarda `> stop + DBL_EPSILON·1e3` (21 pontos) | como o ngspice | o oráculo compara contagens de pontos com o ngspice; `docs/dune-core.md` |
| Passo zero no `.dc` | laço infinito | erro | erro | entrada hostil nunca trava (CLAUDE.md princípio 4) |
