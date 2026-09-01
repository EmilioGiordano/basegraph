# GOAL: `recall` tiene que encontrar la memoria cuyo ancla murió

## El defecto

Descubierto por el piloto 2, registrado en
`supplementary/pilot_seed456/PROBE_PREREGISTRATION.md` §13.3 con transcripts
como evidencia.

En repo_03 (drift de rename, `render_invoice` → `format_invoice`) el brazo A2
llamó `recall` sobre `src/billing.rs`, `issue_number`, `NEXT_INVOICE` y
`format_invoice`. **Los cuatro devolvieron `count: 0`.** Nunca probó
`render_invoice`, el nombre muerto, y no tenía cómo saberlo. La memoria
huérfana nunca se recuperó, así que el clasificador de frescura nunca corrió
sobre ella.

La causa está en el producto. `scope_matches` (`src/mcp.rs:611-616`) es
igualdad exacta de string:

```rust
Scope::File(p) => p == target,
Scope::Symbol(s) => s == target,
```

y `remember` (`src/mcp.rs:427`) siempre guarda `Scope::Symbol(fqn)`. De ahí
salen tres consecuencias, las tres confirmadas en los transcripts:

1. Una memoria anclada a un símbolo es **invisible a una consulta por
   archivo**. En repo_02, `recall("src/logistics.rs")` → `count: 0` mientras
   `recall("lead_time_days")` → la memoria.
2. Después de un rename, la memoria es **invisible a todo nombre que el agente
   pueda conocer**. Solo el nombre muerto la alcanza.
3. `classify` ya sabe resolver el sucesor (`shape_hash` → `format_invoice`),
   pero ese resultado se usa únicamente para **anotar** la memoria una vez
   recuperada. Nunca para **encontrarla**.

Enunciado corto: **el retrieval funciona exactamente cuando el nombre del ancla
no cambió, que es exactamente cuando la frescura no aporta nada.** La capa de
la que trata la tesis está detrás de un lookup que falla antes.

## Qué hay que construir

Tres cambios. El (1) es el que cierra el agujero; (2) y (3) lo hacen usable.

### 1. Buscar por candidato de re-anclaje, no solo por scope

`recall(target)` debe alcanzar una memoria cuando `target` es uno de los
candidatos que `classify` propone hoy para su ancla. Con eso,
`recall("format_invoice")` recupera la memoria anclada a `render_invoice`.

Restricciones, no negociables:

- **Solo bases `SigHash` y `ShapeHash`.** `TokenSimilarity` tiene umbral 0.5
  (`SIMILARITY_THRESHOLD` en `src/memory/anchor.rs`) y convertiría el recall en
  una red que trae cualquier cosa. Que siga sirviendo para *proponer* un
  re-anclaje; no para *buscar*.
- **El hit se marca.** Una memoria alcanzada por candidato se sirve
  explícitamente como tal (p. ej. `reached_via: "reanchor candidate for
  render_invoice"`), nunca indistinguible de un hit directo. La invariante del
  módulo — lo incierto jamás se sirve como confiable — vale también para el
  camino de búsqueda.
- **No re-ancla nada.** `reanchor` sigue siendo la única vía de confirmación.
  Encontrar no es confirmar.

### 2. Retrieval por archivo

Una memoria anclada a un símbolo debe encontrarse consultando el archivo donde
ese símbolo vive **hoy**, resolviendo `fqn → nodo → nodo.file` contra el grafo
vivo en tiempo de lectura.

Para un ancla huérfana sin candidatos el archivo actual es incalculable, y por
eso hace falta el (3).

### 3. `file` en `AnchorKey`, con el patrón que ya usa el repo

Agregar `file: String` a `AnchorKey` (`src/memory/model.rs:16-24`), capturado
en `anchor_of` desde `node.file`.

Seguí el precedente exacto de `shape_hash`: `#[serde(default)]`, para que los
logs escritos antes del cambio sigan cargando, y **un valor vacío nunca
matchea**. Sin bump de `EVENT_VERSION`: es un campo aditivo con default, igual
que el anterior. Las memorias de los pilotos 1 y 2 tienen que seguir
leyéndose — son material suplementario de la tesis.

Con el archivo guardado, una memoria cuyo ancla se volvió inalcanzable sigue
siendo recuperable por el archivo donde se escribió.

## Qué NO se toca

- **`go-no-go.md` sigue sellado.** Ni umbrales ni diseño experimental.
- **Parser, grafo, PageRank, y las tools `map`/`search`/`context`/`show`.**
- **`supplementary/pilot_seed456/`** y el material del piloto 1: congelados.
  Son el registro de un experimento ya corrido.
- **`capture.rs`**: los prompts de siembra de A1 y A2 quedan simétricos.

## Declaración de honestidad, obligatoria

`go-no-go.md` dice no tocar el MCP base. Esto lo toca. La justificación es que
el defecto es un **bug del sistema bajo prueba descubierto por el experimento**,
no un ajuste del experimento para ganar — pero la distinción hay que
argumentarla, no asumirla:

- El defecto se documentó **antes** de decidir arreglarlo
  (`PROBE_PREREGISTRATION.md` §13.3, commit `423d39d`), con el veredicto NO-GO
  del piloto 2 ya firmado y sin ampliación.
- El arreglo **le da a A2 una capacidad que un `gotchas.md` no tiene**: seguir
  el símbolo a través de un rename. Eso es exactamente la hipótesis de la
  tesis, así que cualquier piloto 3 tiene que declarar que el sistema bajo
  prueba cambió entre pilotos, y no puede comparar sus números con los del
  piloto 2 como si fueran el mismo sistema.
- Si se corre un piloto 3, el cambio se declara en su pre-registro **antes** de
  generar materiales nuevos.

## Tests que tienen que existir

Cada uno debe fallar contra el código actual:

1. Memoria anclada a `f` en `src/a.rs`; el símbolo se renombra a `g` con la
   misma forma. `recall("g")` la recupera, marcada como alcanzada por
   candidato, con status `orphaned`.
2. Mismo escenario: `recall("src/a.rs")` la recupera.
3. Memoria anclada a un símbolo intacto: `recall("<archivo del símbolo>")` la
   recupera. Hoy da 0.
4. Un candidato por `TokenSimilarity` **no** alcanza la memoria por búsqueda.
5. Un log de memorias escrito sin el campo `file` sigue cargando, y su `file`
   vacío no matchea ninguna consulta por archivo.
6. El caso de repo_03 como regresión: ancla `render_invoice`, índice con
   `format_invoice`, `recall("format_invoice")` y `recall("src/billing.rs")`
   ambos devuelven la memoria.

## Criterio de aceptación

- `cargo test` verde, `cargo clippy --all-targets -- -D warnings` limpio.
- Ninguna `.unwrap()`/`.expect()`/`panic!` nueva en código de librería.
- El log de memorias del piloto 2
  (`supplementary/pilot_seed456/results/captures/*/a2/codegraph-memory.jsonl`)
  carga sin error contra el binario nuevo.
