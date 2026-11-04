# KNOWN_DIFFS — divergências conhecidas

Divergências deliberadas ou inevitáveis entre Dune, SPICE3f5 e ngspice.
Cada linha aponta a decisão (`DECISIONS.md`) ou a causa. Nunca silenciar
uma divergência por tolerância larga (D-0006).

| Área | 3f5 | ngspice | Dune | Causa / decisão |
|---|---|---|---|---|
| Cabeçalho do rawfile | `raw_read` rejeita `Command:` antes do primeiro `Plotname:` ("Error: misplaced Command: line", `fte/rawfile.c:376`) e descarta a linha | escreve `Command: ngspice-47, Build` entre `Date:` e `Plotname:` em todo rawfile | leitor espelha o 3f5: descarta a linha e registra o aviso `misplaced Command: line` (todo golden gerado pelo ngspice o carrega) | fidelidade ao leitor do 3f5 (D-0002); o aviso é esperado e inofensivo |
| Corpus: `mixdisto.cir` | rodado interativamente com `disto` no prompt (as linhas `*disto` do deck são comentários) | `-b` sem cartão de análise produz rawfile vazio | sem golden; listado em `corpus/SKIP` | deck upstream sem cartão de análise |
