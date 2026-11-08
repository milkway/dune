# Como cortar release dos pacotes Python (`spice3`) e R (`spice`)

Os dois pacotes versionam junto com o repo (0.1.0 = tag `v0.1.0`), mas
cada um tem a sua fonte de versão. Publicar é sempre um passo manual
do mantenedor; nada aqui sobe sozinho a partir de um push.

## Versões — onde mexer

| Pacote | Fonte única | Também |
|---|---|---|
| Python `spice3` | `python/Cargo.toml` `version` (o `pyproject.toml` tem `dynamic = ["version"]`; `spice.__version__` vem do `CARGO_PKG_VERSION`) | `cd python && cargo update -p spice-py --offline` (Cargo.lock); `python/CHANGELOG.md` |
| R `spice` | `r/DESCRIPTION` `Version` | `r/src/rust/Cargo.toml` `version` (mesmo número, por higiene) + `cd r/src/rust && cargo update -p spice --offline`; `r/NEWS.md` |

O `test_version` do pytest compara `spice.__version__` com o
`python/Cargo.toml`, então esquecer o lock/rebuild aparece no teste.

## Python

O sdist compila sozinho: o maturin copia para dentro do tarball as
dependências por caminho (`crates/dune-{core,netlist,raw,pic}`) e o
manifesto do workspace de que elas herdam (`version.workspace` etc.),
mantendo o layout `python/` + `crates/`, e leva o `python/Cargo.lock`.
Crates do crates.io são baixados pelo cargo do consumidor (precisa de
Rust ≥ 1.88). Regra que isso impõe: **nenhum crate do motor pode ler
arquivo fora do próprio diretório** em código de biblioteca
(`include_str!("../../../corpus/...")` quebra o sdist — foi o caso do
prelude `spice.pic`, hoje em `crates/dune-pic/src/`). Em teste
(`#[cfg(test)]`) pode.

Verificação local antes de publicar:

```sh
cd python
maturin sdist -o /tmp/spice3-dist
maturin build --release -o /tmp/spice3-dist
# venv limpa, longe do repo: sdist (compila) e wheel
uv venv /tmp/v-sdist && uv pip install -p /tmp/v-sdist/bin/python /tmp/spice3-dist/spice3-*.tar.gz pytest pandas
uv venv /tmp/v-wheel && uv pip install -p /tmp/v-wheel/bin/python /tmp/spice3-dist/spice3-*.whl pytest pandas
# pytest do repo contra o pacote instalado (rodar de fora de python/,
# senão o `spice/` do fonte, sem o módulo nativo, sombreia o instalado)
PY=$PWD
cd /tmp && /tmp/v-sdist/bin/python -I -m pytest -q -p no:cacheprovider -c /dev/null \
  --rootdir="$PY/tests" "$PY/tests"
cd /tmp && /tmp/v-sdist/bin/python -I -m pytest -q -p no:cacheprovider -c /dev/null \
  --rootdir=/tmp --doctest-modules --pyargs spice
```

Publicação (trusted publishing/OIDC, sem token): publicar a GitHub
release `vX.Y.Z` dispara `.github/workflows/publish-pypi.yml` (ou
`workflow_dispatch` na aba Actions). O workflow builda as wheels abi3
nas 4 plataformas e o sdist (que é instalado numa venv fora do
checkout antes do upload) e sobe tudo com `skip-existing: true` —
redisparar depois de uma falha parcial só completa o que faltou.

## R

Tarball CRAN = fonte do pacote + `src/rust/vendor.tar.xz` com tudo o
que o cargo precisa, no padrão do pacote `rpic` (CRAN): o build roda
**offline**, com no máximo 2 jobs, `CARGO_HOME` e target dentro do
pacote, apagados ao fim. Diferente do rpic (cujo motor está no
crates.io), os crates do Dune não estão publicados; por isso o
arquivo leva duas árvores:

- `vendor/` — `cargo vendor --locked` do `r/src/rust` (os crates do
  crates.io, com checksums);
- `dune/` — `crates/dune-{core,netlist,raw,pic}` (manifesto + `src/`)
  e uma cópia enxuta do `Cargo.toml` do workspace (só
  `[workspace.package]`/`[workspace.dependencies]` e esses 4 membros).

O `Makevars` desempacota as duas em `src/`; o `configure`, ao não
achar `../../../crates` (tarball fora do repo), reescreve os caminhos
do `src/rust/Cargo.toml` para `../dune/crates/`. Sem o arquivo e fora
do repo, o configure falha (ou usa `DUNE_ROOT`, que é o que o CI faz)
— nunca baixa nada às escondidas.

Passo a passo (da raiz do repo):

```sh
tools/r-vendor.sh                 # vendor.tar.xz + tests/testthat/corpus + inst/AUTHORS
git diff r/inst/AUTHORS           # mudou? commitar (lista de crates/autores exigida pelo CRAN)
R CMD build r                     # -> spice_X.Y.Z.tar.gz (~11 MB)
env -u DUNE_ROOT R CMD check --as-cran spice_X.Y.Z.tar.gz
```

O `tools/r-vendor.sh` também copia para `r/tests/testthat/corpus/`
(git-ignorado, vai no tarball) os arquivos do corpus que os testes
leem via `corpus_path()`: fora do repo a suíte roda inteira contra
essa cópia, exceto `test-corpus.R` (o corpus todo, ~110 decks), que
pula sem o repo/`DUNE_ROOT`. Ao acrescentar um `corpus_path(...)` novo
num teste, acrescente o arquivo à lista do script.

Resultado esperado do `--as-cran` (0.1.0, macOS, R 4.6): 0 ERROR,
0 WARNING, NOTEs:

- *CRAN incoming feasibility* — tamanho do tarball (~11 MB, do
  `vendor.tar.xz`, exigido pela política de Rust do CRAN para build
  offline); na submissão real soma-se "New submission".
- *HTML version of manual* — só quando o `tidy` local é o antigo do
  macOS; ambiente, não pacote.

Depois: `cran-comments.md` (em `r/`, fora do tarball) atualizado,
win-builder (`devtools::check_win_devel("r")` — sobe o tarball, ou
seja, é publicação de fato: decisão do mantenedor) e
`devtools::submit_cran("r")`. Não comitar `vendor.tar.xz` nem o
tarball (estão no `.gitignore`).
