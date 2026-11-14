# `.four` — análise de Fourier (harmônicos e THD) de um transiente

Uma camada **pura** de pós-processamento, irmã do `.measure`
(`docs/dune-measure.md`): amostras entram (um vetor de um `Solution`
de `.tran`), uma tabela de harmônicos sai. Nada aqui toca a simulação.

## Contrato e origem

- **Contrato: ngspice-47 em modo batch.** O `.four` do 3f5 vive no
  frontend (`src/lib/fte/fourier.c`, `fte/dotcards.c`); o ngspice
  manteve o algoritmo e acrescentou `nperiods`. Onde os dois diferem,
  segue-se o ngspice (ver abaixo).
- **Traduzido de** (Copyright 1990 Regents of the University of
  California, autor 1985 Wayne A. Christopher, U. C. Berkeley CAD
  Group; Modified BSD — a linha de autoria fica no comentário do
  módulo):
  - `src/frontend/fourier.c` — `fourier` (janela, grade, variáveis,
    impressão) e `CKTfour` (a DFT);
  - `src/maths/poly/interpolate.c` (`ft_interpolate`, `putinterval`),
    `polyfit.c` (`ft_polyfit`), `polyeval.c` (`ft_peval`) — a
    reamostragem polinomial;
  - `src/frontend/points.c` (`ft_minmax`);
  - `src/frontend/dotcards.c` (`ft_cktcoms`, `setcplot`) — o
    enquadramento do batch: o `.four` roda depois da simulação, em
    ordem de deck junto com os `.print`, sobre o plot de transiente
    **mais novo**.
- Variáveis do frontend (`cp_getvar`, que também enxerga os
  `.options` do deck — conferido no ngspice-47): `nfreqs` (10),
  `nperiods` (1), `polydegree` (1), `fourgridsize` (200). A
  **primeira** ocorrência vence (a lista de variáveis do circuito é
  varrida da cabeça); valor real é truncado (`(int) va_real`); valor
  fora da faixa (`nfreqs`, `nperiods`, `fourgridsize` < 1,
  `polydegree` < 0) ou flag nua volta ao default — com aviso no Dune,
  em silêncio no ngspice. `fournosave` (vetores `fourierNM`/`thdNM`
  no plot) não se aplica: o Dune não cria vetores no plot. `numdgt`
  **não** chega ao `cp_numdgt` por `.options` no ngspice; o CLI
  imprime sempre 6 dígitos, como o default.

## Equações

Para um vetor `v(t)` e fundamental `f`, com `P = nperiods`:

1. janela: `[t_max − P/f, t_max]` (`t_min`/`t_max` do `ft_minmax` da
   escala); se o transiente é mais curto que `P/f`, erro
   `(P * wavelength) longer than time span`;
2. grade uniforme de `N = fourgridsize·P` pontos,
   `t_i = t_0 + i·(t_max − t_0)/N`, `i = 0..N−1` (o extremo direito
   não entra); valores por interpolação polinomial deslizante de grau
   `polydegree` (`ft_interpolate`: janela de `grau+1` amostras centrada,
   grau rebaixado onde o ajuste falha pelo critério 1e-3 abs/rel do
   `ft_polyfit`, arestas verticais — duas amostras no mesmo instante —
   viram inclinação recuando o ponto anterior em `1e-3` do passo).
   Grau 1 é a reta fechada `c0 = (x1·y0 − x0·y1)/(x1 − x0)`,
   `c1 = (y1 − y0)/(x1 − x0)`, avaliada como `c0 + c1·t`.
   `polydegree = 0`: sem reamostragem — as amostras cruas, tratadas
   como uniformes, com `N` = comprimento do vetor;
3. DFT (`CKTfour`, ordem de operações do C):
   `s_k = Σ v_i·sin(2πkPi/N)`, `c_k = Σ v_i·cos(2πkPi/N)`,
   `k = 0..nfreqs−1`;
   DC `= c_0/N` (com sinal);
   `|V_k| = hypot(2s_k/N, 2c_k/N)`;
   `φ_k = atan2(2c_k/N, 2s_k/N)·180/π` em graus (referência seno:
   `sin` com fase 0 lê 0°, `cos` lê 90°);
   normalizados `|V_k|/|V_1|` e `φ_k − φ_1` (sem dobrar em ±180°: o
   ngspice imprime −270);
   `THD = 100·sqrt(Σ_{k=2}^{nfreqs−1} (|V_k|/|V_1|)²)` em %.

## API

```
dune-netlist  ir::OutputKind::Four { freq }  (+ Output::vars)
        │
        ▼
dune-core     four::FourParams::from_options(&[Param], &mut diags)
              four::fourier(time, values, fund, &params) -> Result<Fourier, FourError>
              Solution::fourier(fund, &OutExpr, &params)
              four_all(&[Output], &[Solution], &params) -> Vec<FourOutcome>
        │
        └── dune-cli   formato de saída do ngspice (four.rs, só aqui)
```

`Fourier { fund, thd, gridsize, polydegree, nperiods, harmonics }`,
`Harmonic { freq, mag, phase, norm_mag, norm_phase }`. `four_all`
usa o **último** plot de transiente; `all` expande para todo vetor
do plot, escala incluída (como o `ft_evaluate`). Falhas
(`FourError`): `NoTransient`, `UnknownVector`, `LengthMismatch`,
`SpanTooShort`, `Interpolate`, `BadFrequency`, `TooLarge`.

## Saída do CLI (`crates/dune-cli/src/four.rs`)

Fiel ao `fourier()`: `Fourier analysis for <vetor>:`, a linha
`No. Harmonics: …, THD: %g %, Gridsize: …, Interpolation Degree: …,
No. Periods: …`, cabeçalho e linhas com colunas de largura 11
(`fw = numdgt + 5`) e os brancos finais; números pelo `pnum`
(`%.6g`, `%.5g` para negativos). Duas linhas em branco depois de
cada cartão bem-sucedido (`ft_cktcoms`). Erros que fazem o ngspice
abandonar o cartão (`goto done`: sem transiente, janela curta,
interpolação) param o cartão também aqui, com
`No transient data available for fourier analysis` no stderr (o
ngspice não põe `\n` no fim dessa linha; o Dune põe). Vetor
desconhecido só pula aquele vetor. Sem `-r` os cartões saem na ordem
do deck entre os `.print`; com `-r` saem também (depois das medidas)
— o ngspice os ignora com rawfile (KNOWN_DIFFS).

## Entrada hostil

- `.four` sem frequência ou sem vetores, frequência não numérica,
  ≤ 0 ou não finita: erro com span no parser.
- Grade ou trabalho excessivos (`fourgridsize·nperiods` > 10⁷ pontos,
  ou pontos × harmônicos > 10⁹): `FourError::TooLarge` em vez de
  horas de CPU ou gigabytes de alocação.
- Escalas degeneradas onde o C leria fora do vetor (todas as
  abscissas iguais etc.): erro de interpolação, nunca pânico.

## Divergências deliberadas (todas no `corpus/KNOWN_DIFFS.md`)

1. **`nperiods` não é composto por vetor.** No ngspice
   `fourgridsize = fourgridsize * nperiods` fica dentro do laço de
   vetores: com `nperiods = 2` o 1º vetor de um cartão usa 400 pontos,
   o 2º 800, o 3º 1600 (medido no ngspice-47). O Dune usa
   `fourgridsize·nperiods` para todos. Com `nperiods = 1` (default)
   não há diferença; os decks do oráculo com `nperiods = 2` usam um
   vetor por cartão.
2. **`.four` com `-r`** imprime as tabelas (como o `.measure` do Dune).
3. **`all`**: nomes como o Dune os guarda (`v(1)`, `i(v1)`, minúsculos;
   o ngspice imprime `V(1)`, `v1#branch`) e sem as tabelas espúrias de
   vetores de outro comprimento ("lengths don't match").
4. Mensagens no stderr com `\n` final; vetor desconhecido
   (`Error: no such vector v(9)`) reportado na hora do `.four`, não na
   leitura do deck.

O 3f5 difere do ngspice em: sem `nperiods`; `sqrt(tmp²+ph²)` em vez de
`hypot`; cabeçalho sem `No. Periods`; laço infinito num vetor de
comprimento errado (`continue` sem avançar o `v_link2`) — segue-se o
ngspice.

## Validação

- **Unitários** (`crates/dune-core/src/four.rs`): seno puro na grade
  (|V₁| exato, THD < 1e-10), cosseno (90°) e DC com sinal, onda
  quadrada (série `4/(πk)`, THD dos ímpares), janela só no último
  período e `nperiods`, erros (janela curta, frequência ruim,
  comprimentos), tamanhos hostis, interpolação linear/quadrática
  exata, aresta vertical, escalas degeneradas, precedência dos
  `.options`. CLI (`crates/dune-cli/src/four.rs`): `%g` do C, layout
  da tabela contra linhas impressas pelo ngspice-47, quebra do cartão
  nos erros.
- **Oráculo** (`dune-oracle --ci`, seção `.four`; detalhes em
  `docs/dune-oracle.md`): `corpus/four/*.cir` com as tabelas do
  ngspice-47 a 17 dígitos e o rawfile de onde vieram
  (`tools/four-golden.sh`). Duas checagens por tabela — *port* (o
  código do Dune sobre o plot do próprio ngspice, régua de
  arredondamento) e *simulação* (o Dune simula, perfil spice3, régua
  derivada do LTE do transiente). Margens medidas em 2026-11-13: pior
  *port* 2,4e-3 da régua, pior *simulação* 1,5e-3.
