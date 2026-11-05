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
| `TMAX` default do `.tran` | `(tstop−tstart)/50` | `min(tstep, span/50)` (default do SPICE2) | como o ngspice | densidade de pontos do oráculo; `docs/dune-core.md` §4 |
| `.tran … UIC`, ponto t=0 | despeja o vetor zerado | omite o ponto t=0 | como o ngspice | D-0006; `docs/dune-core.md` §5 |
| Sinal do estímulo AC da fonte I | `isrcacld.c` soma com polaridade oposta à do próprio load DC (deslize histórico) | corrigido (mesma polaridade do DC) | como o ngspice | flip de 180° medido no oráculo (`acdiode`); `devices.rs::IsourceI::load_ac` |
| `.options defas` | `cktsopt.c` grava `defas` em `TSKdefaultMosAD` (copy-paste): a área default de source fica inatingível | corrigido (`defas` → AS) | como o ngspice | bug evidente do 3f5; `options.rs` |
| `DEVqmeyer` (capacitâncias de Meyer) | sem clamp; inversão fraca põe toda a capacitância em `capgs` | `vdsat = max(vdsat, 25mV)` (`MAGIC_VDS`) e inversão fraca reparte gs/gd com a suavização de saturação | como o ngspice | caminho de carga do MOS nas transições; `mos1.rs::qmeyer` |
| Diodos de corpo do MOS1/MOS2 | ramo linear `gbs=Is/vt` para `vbs≤0`; gmin fora da corrente; companheiro subtrai gmin | fuga plana `gbs=gmin` abaixo de −3·vt, exponencial clampada acima, `gmin·v` dentro da corrente, companheiro com condutância cheia | como o ngspice | `mos1.rs`/`mos2.rs` (seção *bulk diodes*) |
| Junções do BJT | gmin dentro de `gbe`/`gbc` (amplificado por 1/βR em `gmu`); corte exponencial em −5·n·vt | gmin nos ramos de fuga (`gben`/`gbcn`); cauda polinomial C³ abaixo de −3·n·vt | como o ngspice | sem isso o teste NEWCONV nunca assenta num BJT `off` (regressão `corpus/regress/offbjt.cir`); `bjt.rs` |
| Corte do MOS2 (nível 2) | fronteira em `von` | alansfixes move para `vbin` (gm negativo com corrente nula em `(vbin, von]`) | **mantém o 3f5** (`von`) | correntes idênticas nos dois; o gm negativo do ngspice só converge com o dynamic gmin dele; medido no oráculo (`m2gs`) |
| Gmin stepping do `.op` | escada fixa (`gminsteps` passos ×10) | `gminsteps=1` (default) usa o *dynamic gmin* de Gillespie | como o ngspice (default 1; >1 mantém a escada do 3f5) | schmitt/mosamp2 só fecham o op com ele uma vez que o NEWCONV está ativo; `engine.rs::dynamic_gmin` |
| Source stepping no TRANOP | `srcFact` só escala o valor DC; fontes só-transiente ficam cheias e o stepping nunca zera o circuito | escala o valor final de **toda** fonte sob `MODETRANOP` | como o ngspice | tranop do schmitt (pulse sem DC); `devices.rs::source_value` |
