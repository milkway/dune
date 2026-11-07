# Desempenho — medições registradas

Números de mudanças que mexem no caminho numérico ou no paralelismo
(CLAUDE.md, "Benchmarks"). Cada seção diz a máquina, o comando e a
carga da máquina na hora: as medições de 2026-11 foram feitas com
outros agentes compilando no mesmo Mac, então são pessimistas e
ruidosas (±15% entre rodadas).

## Load paralelo de dispositivos (issue #9, alavanca 1)

`Options::load_threads` (feature `parallel`, default `1`). Desenho e
prova de determinismo em `docs/dune-core.md` ("Load paralelo") e no
cabeçalho de `crates/dune-core/src/parload.rs`.

Comando (melhor de 5, `.tran 0.1n 10n`; cada célula confere
bit-identidade com a coluna de 1 thread antes de imprimir):

```sh
cargo run --release -p dune-core --features parallel --example parload_bench -- \
    10 50 100 200 400 --threads 1,2,4,6,8 --reps 5
cargo run --release -p dune-core --features parallel --example parload_bench -- \
    1 2 5 10 20 40 --width 20 --threads 1,2,4,6,8 --reps 5
```

Máquina: Apple M2 Pro (6P + 4E), 2026-11-07, load average 7–9 de
outros processos durante a medição.

**Cadeia estreita** — N inversores BSIM3 (cartões do
`corpus/regress/bsim3_inv.cir`) em série: 2N MOS sobre ~N nós, então
a LU densa (faer, O(n³)) cresce mais rápido que o load.

| inversores | MOS | 1 thr | 2 thr | 4 thr | 6 thr | 8 thr |
|---|---|---|---|---|---|---|
| 10 | 20 | 3,7 ms | 10,5 ms (0,35×) | 15,8 ms (0,23×) | 18,6 ms (0,20×) | 20,9 ms (0,18×) |
| 50 | 100 | 40,4 ms | 54,0 ms (0,75×) | 66,1 ms (0,61×) | 75,3 ms (0,54×) | 71,3 ms (0,57×) |
| 100 | 200 | 104,6 ms | 124,6 ms (0,84×) | 117,4 ms (0,89×) | 139,3 ms (0,75×) | 156,6 ms (0,67×) |
| 200 | 400 | 342,1 ms | 353,5 ms (0,97×) | 312,6 ms (1,09×) | 330,8 ms (1,03×) | 360,1 ms (0,95×) |
| 400 | 800 | 1572 ms | 1570 ms (1,00×) | 1501 ms (1,05×) | 1526 ms (1,03×) | 1534 ms (1,03×) |

**Cadeia larga** (`--width 20`) — 20 inversores em paralelo por
estágio: 40·N MOS sobre só N + 2 nós, o load domina a LU.

| estágios | MOS | 1 thr | 2 thr | 4 thr | 6 thr | 8 thr |
|---|---|---|---|---|---|---|
| 1 | 40 | 5,5 ms | 8,3 ms (0,66×) | 11,2 ms (0,49×) | 12,7 ms (0,43×) | 14,5 ms (0,37×) |
| 2 | 80 | 9,9 ms | 11,5 ms (0,87×) | 13,7 ms (0,72×) | 16,2 ms (0,61×) | 15,4 ms (0,64×) |
| 5 | 200 | 30,5 ms | 34,2 ms (0,89×) | 30,6 ms (0,99×) | 33,8 ms (0,90×) | 33,6 ms (0,91×) |
| 10 | 400 | 75,7 ms | 70,0 ms (1,08×) | 61,1 ms (1,24×) | 65,2 ms (1,16×) | 56,8 ms (1,33×) |
| 20 | 800 | 222,8 ms | 162,0 ms (1,38×) | 135,2 ms (1,65×) | 141,2 ms (1,58×) | 137,3 ms (1,62×) |
| 40 | 1600 | 611,8 ms | 443,6 ms (1,38×) | 336,3 ms (1,82×) | 306,1 ms (2,00×) | 326,2 ms (1,88×) |

Leitura:

- **Limiar**: ~300 instâncias BSIM3 *com o load dominando* para
  empatar; abaixo disso o paralelo perde (0,2–0,9×). Os decks do
  corpus (≤ ~50 dispositivos) ficam todos abaixo — por isso o default
  é `1`.
- **Custo fixo por load** ≈ 15–25 µs: acordar os workers do rayon a
  cada iteração de Newton (eles dormem durante a LU) mais a montagem
  das janelas. Um load BSIM3 custa ~0,4 µs por instância, então são
  necessárias centenas delas para pagar a viagem.
- **Replay serial**: ~13% do load sequencial (uma soma por carimbo,
  ~26 carimbos por BSIM3). É o preço da bit-identidade; um replay
  paralelo por faixa de posições (cada posição em ordem de chunk)
  é possível e fica como próximo passo se o load virar gargalo.
- **Amdahl com a LU densa**: na cadeia estreita de 400 inversores o
  load é só ~16% da iteração (302 µs de ~1,9 ms); a LU densa domina e
  nenhum número de threads passa de ~1,1×. A alavanca 1 rende de
  verdade quando o backend esparso (P-3/D-0013) tirar a LU do caminho
  — e com os outros laços por dispositivo (`conv_test`, `trunc`,
  `accept`), ainda sequenciais, paralelizados do mesmo jeito.
- Teto observado: 2,0× em 6 threads (1600 BSIM3, load dominante).
  Os núcleos E e a carga de outros processos achatam a curva acima
  de 4 threads.

Antes do paralelo, uma contenção escondida: o load do BSIM3 clonava
três `Arc` (modelo, temperatura, tamanho) por chamada — um incremento
atômico numa linha de cache que todas as instâncias do modelo
compartilham. Em paralelo isso serializava os workers (soma dos
tempos de job 3× o load sequencial); agora o load empresta os
cartões. Os logs de carimbo por worker também ficam alinhados a 128
bytes (sem *false sharing* no `len` dos `Vec`).

O caminho sequencial (`load_threads = 1`) não perdeu nada mensurável:
`dune -b` na cadeia de 100/400 inversores, 0,15 s / 1,70–1,89 s antes
e depois (ruído da máquina); rawfile e stdout dos 88 decks do corpus
(examples, basic, regress, measure, pss) **byte-idênticos** ao main.
