# corpus/

Material de referência e de teste. Nada aqui é editado localmente: upstream
fica intocado; anotações e diferenças vão em `KNOWN_DIFFS.md`.

## spice3f5/

Fonte completa do SPICE3f5 de Berkeley (licença BSD, ver `COPYRIGHT`).

- Origem: <https://ptolemy.berkeley.edu/projects/embedded/pubs/downloads/spice/spice3f5.tar.gz>
- Baixado em: 2026-11-03
- SHA-256 do tarball: `cac11fe2a761241e6b6c9eaa31b938c7ffa76aeaecac09809609d3a4125cd269`
- Manual (contrato de sintaxe): `spice3f5/lib/helpdir/spice.txt`
- Circuitos de exemplo do upstream: `spice3f5/examples/*.cir` (20 circuitos,
  cobrindo `.op/.dc/.ac/.tran/.pz/.disto/.noise`)

## berkeley/ (pendente)

O `PLAN.md` prevê `quarles_bench.tar.gz` (benchmarks do memo ERL M89/47) e
`qxdir.tar.gz` (circuitos e `.bug` usados em Berkeley para validar
distribuições). Em 2026-11-03 esses arquivos **não estão mais disponíveis**:
o `README` do diretório de Berkeley ainda os lista, mas as URLs dão 404; o
espelho `gigascale.org` está morto; não há captura no Wayback Machine nem
espelho conhecido no GitHub. Pistas para retomar a busca:

- Internet Archive estava fora do ar durante a busca — tentar de novo
  (`web.archive.org/cdx` para `ic.eecs.berkeley.edu/pub/Spice3/`).
- A suíte de regressão do ngspice (`tests/` no git do SourceForge) declara
  derivar das duas suítes: <https://ngspice.sourceforge.io/bench.html>.
- Contato histórico: `spice@eecs.berkeley.edu`.

Enquanto isso, o corpus inicial do oráculo são os `spice3f5/examples/` e, se
preciso, circuitos da suíte do ngspice (com proveniência anotada aqui).

## basic/

Decks mínimos escritos para o Dune (não vêm de Berkeley; proveniência
anotada em cada arquivo): um por recurso da Fase 1, todos `.op`, para
paridade com o ngspice enquanto `quarles_bench`/`qxdir` não aparecem.

## ORACLE

Versão do ngspice usada para gerar `golden/` (fixada; regenerar o golden
exige a mesma versão ou uma entrada em `KNOWN_DIFFS.md`).

## golden/

Rawfiles do ngspice, versionados, um por circuito do corpus
(`<stem>.raw`, ASCII). Gerados com
`cargo run -p dune-oracle -- --gen-golden`, que exige o ngspice de
`ORACLE` instalado. Regenerar com outra versão exige repinar `ORACLE` e
anotar em `KNOWN_DIFFS.md`.

## XFAIL

Comparações conhecidamente marginais (`stem|plot|teto|motivo`): rodam
e aparecem em todo relatório; XFAIL dentro do teto não derruba o CI,
estourar o teto derruba, e passar pede a remoção da entrada. Ver
D-0011.

## SKIP

Circuitos que o oráculo não roda: `<caminho relativo a corpus/> <motivo>`,
um por linha, `#` comenta. A história de cada entrada fica em
`KNOWN_DIFFS.md`.

## regress/

Um `.cir` mínimo por bug corrigido (regra do `CLAUDE.md`).
