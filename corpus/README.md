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

## ORACLE

Versão do ngspice usada para gerar `golden/` (fixada; regenerar o golden
exige a mesma versão ou uma entrada em `KNOWN_DIFFS.md`).

## golden/ (gerado na Fase 0)

Rawfiles do ngspice, versionados, um por circuito do corpus.

## regress/

Um `.cir` mínimo por bug corrigido (regra do `CLAUDE.md`).
