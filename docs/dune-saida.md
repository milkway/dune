# Saída de resultados — seleção × formatação

Como o `.print` foi implementado e por que a mesma camada serve os
pacotes R, Python e JS (PLAN §2.6, D-0008).

## O princípio

Nenhum consumidor raspa texto formatado. A cadeia é:

```
Solution (espelho do rawfile: nomes, VarType, colunas f64, escala)
   │
   ├── Solution::select(&[OutExpr]) → Selection      ← SELEÇÃO (dune-core)
   │     resolve "v(2)", "v(a,b)", "vm/vp/vr/vi/vdb(x)", "i(vin)",
   │     "all", nome cru; deriva colunas (diferença, módulo, fase
   │     em radianos, dB); devolve dados puros + a escala do plot
   │
   ├── dune-cli `.print`  → formato de colunas do spice3 (só aqui)
   ├── dune-py            → polars / numpy zero-cópia das colunas
   ├── dune-r             → tibble longa das colunas
   └── dune-wasm          → RawfileJson {variables, columns}
```

Uma única fonte de verdade: o número que o `.print` imprime é o
mesmo `f64` que o numpy vê, que a tibble carrega e que o JSON
serializa. Formatação é um *renderer* exclusivo do CLI.

## A superfície (dune-core)

- `OutExpr` — expressão de saída: `Voltage{node, reference, part}`,
  `Current{source, part}`, `Name`, `All`. Constrói-se por
  `OutExpr::parse("vm(out)")` (o que os bindings expõem como
  `result["v(2)"]` / `dune_select(res, "v(2)")`) ou por
  `OutExpr::from_ir(&ir::OutVar)` (o que os cartões `.print`/`.save`
  entregam).
- `Part` — `r i m p db` (fase em **radianos**, default do 3f5;
  graus são açúcar dos bindings). Sobre dados reais: `re=x`, `im=0`,
  `mag=|x|`, `ph=atan2(0,x)`, `db=20·log10|x|` — a semântica
  estende-se sem mudança de API quando o `.ac` trouxer colunas
  complexas.
- `Selection { scale: Option<OutColumn>, columns: Vec<OutColumn> }`
  com `OutColumn { label, unit: VarType, data: Vec<f64> }`. A escala
  (`time`, `v(v-sweep)`) sempre acompanha, pedida ou não — é o que um
  binding precisa para montar a tabela longa
  (`time | variable | value`) sem lógica própria.

### Mapa por linguagem (Fase 4)

| Consumidor | O que recebe | Custo de conversão |
|---|---|---|
| Python | `OutColumn.data` → numpy (zero-cópia via PyO3) → polars | nenhum: `Vec<f64>` é o buffer |
| R | `OutColumn.data` → `REALSXP` → tibble longa (`scale` repetida + `label` como coluna `variable`) | uma cópia na fronteira R |
| JS/WASM | `Selection` → `{variables:[{name,unit}], scale, columns: Float64Array[]}` | `Float64Array` view do heap wasm |
| CLI | `print.rs` formata a `Selection` | só texto |

## O formato do `.print` (dune-cli/src/print.rs ← `fte/postcoms.c`)

Porte fiel do modo coluna do `com_print`: blocos por página de
largura (`Index` = 8 colunas, 16 por vetor real, mínimo de 2 vetores
por bloco), escala prefixada a cada bloco, título centrado, linha
`plotname  date` indentada com o *padding do título* (o C calcula um
recuo novo e não o usa — quirk mantido), régua de traços, cabeçalho
`Index` com nomes `%-16.15s`, valores `%e` separados por tab,
paginação a cada `height` linhas com form feed. `width` vem de
`.width out=` (default 80/60, `ftedefs.h`); `.options nopage`
desliga a paginação.

Como no `spice3 -b`: com `-r` o resultado vai para o rawfile e os
`.print` não são renderizados; sem `-r`, os `.print` saem no stdout
(e um deck `.op` sem `.print` ainda lista seus valores).

## Medidas (`.measure`)

A mesma cadeia vale para as medidas escalares: `Solution::measure`
(dune-core) → `Measurement`, renderizado no formato do ngspice só no
CLI; os bindings recebem os números. Ver `docs/dune-measure.md`.

## Pendências

- `.plot` (gráficos ASCII do `ft_agraf`) — aviso e sugestão de
  `.print`; portar se houver demanda real.
- `.save` restringindo o conjunto salvo (hoje salva-se tudo; o
  rawfile maior é compatível, o oráculo trata extras como nota).
- Partes sobre colunas complexas chegam com o `.ac` (a API não muda:
  `Part::apply` ganha a variante complexa).
