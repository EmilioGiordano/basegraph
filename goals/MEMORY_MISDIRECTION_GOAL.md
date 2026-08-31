# GOAL: Memoria stale ENGAÑOSA en drift — spec del fix B2 (solo diseño)

## Contexto

Hallazgo B2, registrado en `supplementary/pilot_seed456/PROBE_PREREGISTRATION.md` §11.2:
A1 (`gotchas.md`) y A2 (memoria anclada) reciben el **mismo payload semántico** y solo
difieren en el direccionamiento. `CAPTURE_C2_A1` pide "one line per gotcha, reference the
symbol (fqn)"; `CAPTURE_C2_A2` pide la invariante en lenguaje natural anclada al símbolo.
Resultado en repo_03: la línea de A1 será del tipo
`render_invoice — never advances NEXT_INVOICE; previews must not consume numbers`.
En C3 `render_invoice` ya no existe, pero la línea nombra `NEXT_INVOICE` (que sobrevive) y
enuncia la regla entera: A1 la aplica igual, sin necesitar el ancla. La condición (b) de
`go-no-go.md` §7 (A2 viola **estrictamente menos** que A1 en drift) queda casi sin margen
por construcción: el único mecanismo vivo es que A1 descarte la nota como documentación
muerta, y colgar un GO de ese coin-flip conductual con n=4 no es sólido.

**Este goal es solo diseño.** Cero cambios de código, cero regeneración de materiales.
`supplementary/pilot_seed456/` está **congelado**: hay un probe corriendo sobre esos
materiales (§11.1 decide con su resultado si estos cambios se implementan o no).

---

## 1. Diagnóstico: qué aporta la frescura del ancla que el texto no aporta

Una memoria es un par **(dirección, payload)**: el ancla (`fqn` + `sig_hash` +
`shape_hash`) y la regla en lenguaje natural. La clasificación en vivo del ancla
(`intact` / `evolved` / `orphaned` + candidatos) aporta exactamente tres cosas que el
payload no puede aportar por sí mismo:

1. **Validez**: si el mundo para el que se escribió la regla sigue existiendo. Un texto
   no sabe que quedó viejo; el ancla sí.
2. **Referente**: a **qué símbolo actual** aplica la regla hoy. Los candidatos de
   re-anclaje (por `sig_hash`/`shape_hash`) resuelven la dirección post-drift.
3. **Calibración**: lo incierto nunca se sirve como `intact`, lo que fuerza verificación
   antes de actuar (la métrica de falsa confianza mide precisamente esto).

En los materiales actuales ese aporte marginal es ~cero, porque el payload
**sobredetermina la aplicación**: nombra un colaborador estable (`NEXT_INVOICE`,
`first_gap`, `DEFAULT_TIMEOUT_MS`) y enuncia la regla completa, así que cualquier lector
la aplica bien sin resolver el referente. Formalmente: el experimento mide el valor del
ancla **solo si** la aplicación correcta *requiere* el referente actual, es decir, si el
payload **subdetermina** la aplicación tras el drift. Hay exactamente tres maneras de que
la subdetermine, y cada una da un escenario de la sección 2:

| Mecanismo | El payload solo… | Escenario |
|---|---|---|
| **Ambigüedad de referente** | …no dice a cuál de varios sucesores aplica | (a) split |
| **Falsificación de referente** | …resuelve textualmente al símbolo equivocado | (b) delete con homónimo |
| **Pérdida de referente** | …no nombra ningún identificador que sobreviva | (c) sin identificador estable |

La memoria stale debe ser **activamente engañosa** (aplicarla al símbolo equivocado
produce un fix incorrecto detectable por el oracle), no meramente mal direccionada.

---

## 2. Escenarios de drift con memoria engañosa

Regla transversal (protocolo §3, sellado): el drift es **sintáctico** — mueve, parte o
renombra el símbolo **sin tocar la semántica del sistema**. Ninguno de estos escenarios
cambia lo que el código hace en C3; cambian *dónde vive* la responsabilidad.

### (a) Drift **split** — el símbolo se parte en dos sucesores y la regla vale para uno

- **Qué emite el generador** (ej. sobre `no_side_effect`/billing): en C3,
  `render_invoice` se parte por refactor en `render_preview` (camino de solo lectura,
  conserva la firma vieja → `sig_hash` matchea) y `render_and_register` (camino de
  emisión, que **legítimamente** consume `issue_number()` al finalizar — comportamiento
  que en C2 vivía en el caller `Invoice::issue`, movido, no cambiado). Las tareas tocan
  ambos caminos.
- **Por qué A1 falla**: la nota dice "nunca avanza `NEXT_INVOICE`". Grep de
  `NEXT_INVOICE` encuentra los dos sucesores. La lectura natural de la nota
  ("never advances") empuja a mantener puro también el camino de registro → el fix
  "obvio" reutiliza un número peekeado o no lo consume → números duplicados/ausentes.
- **Por qué A2 acierta**: `recall` → `orphaned` con candidato por `sig_hash` =
  `render_preview` **solamente** (el otro sucesor cambió de firma). La regla queda
  acotada al sucesor correcto.
- **Oracle**: dos aserciones — el preview repetido no consume números
  (`NEXT_INVOICE` estable) **y** el camino de registro emite números únicos y
  crecientes. El fix de A1 mal aplicado rompe la segunda; ignorar la regla rompe la
  primera.

### (b) Drift **delete con homónimo** — el nombre viejo sobrevive en otro módulo con otra semántica

- **Qué emite el generador** (ej. sobre `sorted_output`/scheduling): en C3 el proveedor
  renombra `merge_windows` → `coalesce_requests` (misma firma → candidato por
  `sig_hash`), y otro módulo gana un homónimo **como método**,
  `PaneLayout::merge_windows` (UI: fusiona paneles preservando el **orden de inserción**;
  su propia suite fija ese contrato). El método es necesario: con fqns pelados, un
  homónimo función libre colisionaría con el ancla y `classify` daría un falso `evolved`
  — restricción de diseño que se registra como hallazgo para el backlog del producto.
- **Por qué A1 falla**: la nota dice "`merge_windows` — output must stay sorted;
  `first_gap` depends on it". Grep de `merge_windows` da un único hit plausible: el
  homónimo. O bien "aplica" la regla ahí (ordenar los panes → rompe su contrato de orden
  de inserción → su primary falla), o bien concluye que la regla no concierne a la tarea
  y deja sin proteger a `coalesce_requests` → el fix obvio de la tarea rompe el orden →
  oracle falla.
- **Por qué A2 acierta**: `recall` → `orphaned`; el candidato por `sig_hash` es
  `coalesce_requests` (el homónimo-método ni aparece: otra firma, otro shape).
- **Oracle**: orden del schedule de `coalesce_requests` (como hoy) + la suite propia del
  homónimo (orden de inserción preservado) corre en la verificación del generador para
  garantizar que "ordenarlo" es un fix incorrecto detectable.

### (c) Invariante **sin identificador estable** — sin el ancla, la regla es inaplicable

- **Qué emite el generador** (ej. sobre `idempotence`/paths): la invariante enuncia una
  propiedad de la función misma, sin colaboradores globales:
  "`normalize_path` — normalizar dos veces es igual que normalizar una; ingest y lookup
  normalizan por separado". En C3, rename a `canonical_path` **y** el proveedor convive
  con dos vecinos plausibles de firma parecida cuyo contrato es legítimamente no
  idempotente: `display_path` (trunca con `…` en cada pasada) y `relative_path`
  (pela un prefijo por llamada). Sus suites propias fijan esos contratos en C1.
- **Por qué A1 falla**: la nota no nombra nada que sobreviva — `normalize_path` no
  existe y no hay un `NEXT_INVOICE` que grepear. Para aplicarla hay que **adivinar** el
  referente entre tres funciones de forma similar; el empate se rompe mal: volver
  idempotente a `display_path`/`relative_path` rompe sus primaries, y no aplicarla deja
  el fix obvio de la tarea rompiendo la idempotencia de `canonical_path` → oracle.
- **Por qué A2 acierta**: candidato por `shape_hash` = `canonical_path` únicamente.
- **Oracle**: `f(f(x)) == f(x)` sobre `canonical_path` (como hoy) + las suites de los
  vecinos como tests propios del repo (la verificación exige que sigan verdes con el fix
  correcto).

**Cuarto candidato evaluado y descartado**: "regla invertida en el sucesor" (el sucesor
tiene el contrato opuesto, p. ej. orden descendente). Se descarta porque exige un drift
**semántico**, y §3 del protocolo sellado exige drift sin cambio de semántica.

En los tres escenarios A0 queda simétrico: la verdad sigue siendo redescubrible leyendo
consumidores/callers o por arqueología de commits, al costo de siempre.

---

## 3. Cambios necesarios (nivel spec, no implementar todavía)

| Archivo | Cambio |
|---|---|
| `tools/common/schema.rs` | `DriftKind::Split` nuevo; `Delete` gana la variante con homónimo (campo `homonym: Option<…>` en el spec del repo o kind `DeleteWithHomonym`). Manifest: `successors: Vec<fqn>` + `rule_applies_to: fqn` (para la rúbrica y el scoring), y el fqn del homónimo cuando exista. |
| `tools/synth_repo_gen/scenarios.rs` | Por escenario: plantillas de **sucesores** (split: dos fns C3 + consumidor actualizado), plantilla de **homónimo** (método en módulo ajeno, con doc y suite propia que fija su contrato), plantillas de **vecinos plausibles** para (c). Los `invariant_text` de los escenarios tipo (c) se reescriben para no nombrar colaboradores estables. Tareas y fixes (correcto/obvio) renderizados contra los sucesores. |
| `tools/synth_repo_gen/render.rs` | Render de proveedor con N sucesores; render del módulo homónimo; consumidor parametrizado por sucesor. |
| `tools/synth_repo_gen/assemble.rs` | Aplicar los kinds nuevos en C3 (split reparte el módulo; homónimo aterriza en un módulo existente o nuevo); poblar los campos nuevos del manifest; **verify extendido**: pristine/correcto/obvio como hoy **más** las suites propias de homónimos y vecinos en verde con el fix correcto, y en rojo con la mala aplicación de la regla (el engaño tiene que ser detectable por construcción, no por suerte). |
| `tools/experiment_runner` | Sin cambios de lógica; opcionalmente registrar qué archivos editó el agente (ya se infiere del transcript) para la revisión con rúbrica. |
| `capture.rs` (prompts) | **Sin cambios**: el payload sigue simétrico entre A1 y A2. El fix es de materiales — la asimetría legítima es que el payload subdetermine, no que A2 reciba más información. |

Los materiales nuevos salen con **seed nueva y batch nuevo** (`pilot_seed<nuevo>`);
`pilot_seed456` no se regenera.

---

## 4. Qué NO cambia

- `go-no-go.md` sellado: ni umbrales de §7 ni diseño experimental (3 brazos, coaching
  simétrico, caps iguales). Esto es corrección de **materiales**, admisible por §0.
- Parser, grafo, PageRank, cache y MCP base intactos. Consecuencia aceptada: los
  homónimos deben ser métodos para no colisionar en el espacio de fqns pelados; la
  cualificación por módulo del indexador queda como hallazgo para el backlog del
  producto, fuera de este experimento.
- `supplementary/pilot_seed456/` congelado hasta que el probe termine; la rama de
  decisión de §11.1 manda: estos escenarios se implementan solo si el probe cae en una
  rama que exige endurecer los materiales de drift.
- Los prompts de captura y el coaching de tarea: simétricos, como están.

---

## 5. Riesgos: el fallo simétrico (fabricar escenarios para que A2 gane)

Volver la memoria engañosa tiene un espejo: diseñar trampas que solo el sistema puede
esquivar convierte el experimento en promoción (§6 del protocolo: "sin el segundo
término el experimento es promocional, no decisivo").

**Defensa de realismo, por escenario:**
- *(a) split*: extraer el camino de preview del camino de commit es de los refactors más
  comunes que existen (separar lectura de escritura); la regla que valía para el todo y
  ahora vale para una mitad es el caso típico de doc desactualizada.
- *(b) homónimo*: los nombres genéricos (`merge_windows`, `render`, `process`) colisionan
  en cualquier codebase mediano; el falso positivo de grep es un modo de fallo real y
  documentado del flujo "buscar el símbolo que nombra la nota".
- *(c) sin identificador estable*: idempotencia, pureza y precondiciones son propiedades
  **de la función misma**; su enunciado natural no nombra colaboradores. No hay que
  forzar la redacción: hay que dejar de regalar el colaborador global en el diseño del
  módulo.

**Guardrails pre-registrables (van al pre-registro del batch nuevo, antes de correr):**
1. **A0 sigue vivo**: en cada escenario la verdad es alcanzable sin memoria — leyendo el
   consumidor/callers o la historia. Si un escenario solo es resoluble vía `recall`, está
   mal diseñado y se poda.
2. **La nota es descartable**: el ancla muerta de A1 es *visiblemente* muerta (grep del
   fqn viejo no resuelve en `src/`); un A1 diligente puede notarlo, descartar la nota y
   caer al camino A0. El engaño castiga aplicar sin verificar, no verificar.
3. **Verify simétrico**: el generador demuestra mecánicamente que la mala aplicación
   rompe tests *del propio repo* (no solo el oracle oculto) — el castigo existe en el
   mundo del agente, no solo en el del experimentador.
4. **Todos los repos embarcados cuentan**: nada de podar escenarios después de ver
   resultados; la selección queda fijada en el pre-registro del batch.

**Señales de que nos pasamos al otro extremo (monitorear en el pilot del batch nuevo):**
- A1 viola **más que A0** en drift de forma sistemática *aun cuando verificó* (leyó el
  código actual y igual falló): la trampa no castiga la confianza ciega sino la tarea
  misma.
- `fix_pass` de A0 cae por debajo del guard de §11.1 (≥5/6): subimos dificultad global,
  no localidad de la trampa.
- A2 gana pero con falsa confianza ≈ A1: el ancla direcciona bien y sin embargo el agente
  no verifica — la ganancia sería de retrieval, no de frescura, y (b) de §7 no debería
  acreditarse a la tesis.

---

## Referencias

- `supplementary/pilot_seed456/PROBE_PREREGISTRATION.md` §11 (enmienda pre-corrida; §11.2 es el hallazgo que motiva este goal)
- `go-no-go.md` §0, §3, §6, §7 (protocolo sellado)
- `tools/experiment_runner/capture.rs` (`CAPTURE_C2_A1` / `CAPTURE_C2_A2`)
- `tools/synth_repo_gen/scenarios.rs` (catálogo actual)
- `supplementary/pilot_seed123/PILOT_REPORT.md` (por qué las trampas locales no discriminan)
