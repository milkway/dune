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

## measure/

Decks com cartões `.measure` (issue #11) e, em `measure/golden/`, o
plot que o ngspice-47 calculou para cada um (`<deck>.raw`) e a saída
das medidas dele com 15 dígitos (`<deck>.meas`; `FAILED <nome>` para
as que ele não mede). Gerados por `tools/measure-golden.sh`. **Não**
são raízes do oráculo (`--ci`): `crates/dune-core/tests/measure_ngspice.rs`
mede o rawfile do ngspice com a camada do Dune e compara com a
saída dele — código de medida contra código de medida, nos mesmos
dados.

## Proveniência e licenças

Resumo de onde vem cada parte do corpus (os avisos de copyright e de
licença completos estão no `NOTICE` da raiz do repositório):

- `spice3f5/`: distribuição original de Berkeley, sem alterações —
  © 1985–1991 The Regents of the University of California, licença em
  `spice3f5/COPYRIGHT`.
- `golden/*.raw`, `measure/golden/*.raw`, `measure/golden/*.meas` e
  `pss/golden/*.raw`: **saídas do ngspice-47** (versão em `ORACLE`)
  rodando os decks deste corpus; não são resultados do Dune. O ngspice
  é do time ngspice e colaboradores (licença "Modified BSD",
  <https://ngspice.sourceforge.io/>). As referências escritas pelo
  próprio Dune são `regress/golden/*.dune.raw` (recursos sem
  equivalente no ngspice) e `pic/golden/*.cir` (netlists extraídas das
  figuras).
- `regress/bsim3_*.cir`: usam os cartões de modelo BSIM3v3.3.0 de
  0,35 µm distribuídos com o ngspice-47 (`examples/mos/modelcard.nmos`
  e `modelcard.pmos`; BSIM3 é do BSIM Research Group da UC Berkeley,
  © Regents of the University of California).
- `regress/vdmos_thermal_*.cir`: o cartão IRFP240 do exemplo
  `examples/vdmos/self-heating.sp` do ngspice-47 (valores térmicos e
  dissipador alterados, anotado nos decks); os demais `regress/vdmos_*`
  foram escritos para o Dune.
- `pss/osc_colpitts.cir`: circuito e cartão `.pss` do exemplo
  `examples/pss/colpitt_osc_pss.cir` do ngspice-47 (anotado no deck).
- `regress/ltra_*_gear.cir`, `mosamp2_fine.cir`, `schmitt_fine.cir`,
  `mixdisto_*.cir`: variantes dos exemplos de `spice3f5/examples/`
  (mesmo circuito, passo ou análise diferentes).
- Demais decks de `basic/`, `regress/`, `measure/`, `pss/` e `pic/`:
  escritos para o Dune. Nem todo deck de `regress/` traz linha de
  proveniência; na falta dela, vale esta lista.

O Dune é um projeto independente, sem vínculo com a University of
California, Berkeley nem com o projeto ngspice, e sem endosso deles.
