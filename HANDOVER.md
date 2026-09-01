# HANDOVER — CodeGraph Thesis Project

**Fecha:** 2026-08-31
**Rama:** `feat/generator-redesign` (por delante de `main`)
**Estado:** piloto 2 corrido y cerrado — **NO-GO**, sin ampliación. Defecto de
retrieval descubierto por el experimento y ya arreglado. Decisión abierta: cerrar
con el negativo o re-pilotear.

---

## 1. Qué es este proyecto (en 3 líneas)

CodeGraph es una herramienta Rust + MCP que indexa codebases y sirve **memoria
anclada a símbolos** con clasificación de frescura en vivo
(`intact`/`evolved`/`orphaned`). Objetivo de tesis: **demostrar empíricamente**
que esa memoria reduce violaciones de invariantes ocultas frente a `grep` y
frente a un `gotchas.md`, cuando el código sufre **drift** (renames, moves,
cambios de firma).

El experimento está **pre-registrado y sellado** en `go-no-go.md` (v2). La
infraestructura completa existe: generador de repos sintéticos, runner de 3
brazos, scorer con veredicto GO/NO-GO.

---

## 2. Estado real hoy

| Bloque | Estado |
|--------|--------|
| Parser + grafo + PageRank | Estable |
| Memoria anclada + clasificador de frescura | Completo (`intact`/`evolved`/`orphaned` + candidatos de re-anclaje) |
| Ciclo de vida | `remember` → `recall` (con frescura) → `reanchor` → `supersede` → `generate_test` |
| MCP server | 9 tools (`map/search/context/show` + `recall/remember/reanchor/supersede/generate_test`) |
| Infra experimento | Generador, runner 3 brazos, scorer |
| Suite | **206 tests, 0 fallos**; `cargo clippy --all-targets -D warnings` limpio |
| Piloto 1 (seed 123) | Corrido — **GREY → NO ESCALAR** (0 violaciones en 18, trampas demasiado locales) |
| Rediseño del generador (6 fixes) | Implementado y commiteado |
| Materiales del piloto 2 (`supplementary/pilot_seed456/`) | Generados, `--verify` 45/45, generación determinista, drift visible end-to-end |
| Probe A0 (compuerta) | Corrido: a0 1/6, drift 1/4, fix_pass 6/6 → SEGUIR |
| Piloto 2 completo | 18 corridas, $11.97 — **NO-GO** por la condición (b); ampliación §7 **no tomada** per §12.5 |
| Fix de retrieval (`recall`) | Implementado y verificado end-to-end contra el material del piloto |

Verificado en esta sesión: `--verify` 45/45; manifest y `verify_report.json`
regenerados salen idénticos a los commiteados; drift end-to-end
(`remember`@C2 → `recall`@C3) da repo_01 `intact`, repo_02 `evolved`,
repo_03 `orphaned` + candidato `format_invoice`; ensayo del harness con agente
scripted ($0) discrimina 6/6 en ambas direcciones; el resume del runner no
re-ejecuta corridas A0 existentes.

---

## 3. Qué hizo el rediseño (6 commits)

| Commit | Cambio | Qué arregla |
|--------|--------|-------------|
| `4bb5e7f` | **Invariantes no locales** — consumidor en módulo separado que importa del proveedor | `grep` en el proveedor ya no ve la dependencia |
| `4bb5e7f` | **Fix "obvio" = camino natural que rompe** — proveedores sin pistas locales | El fix obvio sí rompe la invariante |
| `4bb5e7f` + `5cc4ce7` | **Ruido `--noise-commits 10`** — 8-12 commits entre C1 y C2 | El fix C2 queda enterrado; `git log` sale caro |
| `5cc4ce7` | Quitar drift `duplicate`, reemplazar por `delete` | `duplicate` clasificaba `intact`; `delete` da ancla huérfana real |
| `4063bd9` | **Ventana de verificación FC** — leer el archivo anclado en las 3 llamadas previas al `recall` stale cuenta como verificación | Elimina el falso positivo del piloto 1 |
| `23b7899` | `remember` sugiere el nombre indexado cuando el fqn viene calificado | Fricción vista en las capturas del piloto 1 |

---

## 4. Hallazgos abiertos (leer antes de tocar nada)

**A — Operativo, silencioso y caro.** `experiment_runner` resuelve
`codegraph.exe` como su hermano de directorio y solo comprueba que el archivo
exista. `cargo build --release --bin experiment_runner` **no** recompila
`codegraph.exe`; uno stale responde `unknown tool: remember` y **anula el brazo
A2 sin ningún error visible**. Siempre `cargo build --release` completo.

**B — Diseño: las trampas siguen siendo locales.** El commit C2 deja su test de
regresión *dentro* del archivo que el agente edita
(`short_express_routes_promise_next_day` en repo_02,
`previewing_a_draft_is_stable` en repo_03), y los mensajes de C2 son
autodiagnósticos. repo_03 repite el escenario que ya midió 0 en el piloto 1.
La única trampa candidata a fuerte es repo_01, que es el repo **sin** drift —
y su estado inicial ya es total por construcción, así que el fix mínimo
preserva la invariante sin necesidad de saberla.

**B2 — El payload de la memoria es independiente del ancla.** `capture.rs` le
da a A1 y A2 el mismo contenido semántico; solo cambia el direccionamiento. La
línea de A1 en repo_03 nombra `NEXT_INVOICE` y enuncia la regla entera, así que
el ancla muerta (`render_invoice`) casi no le cuesta nada. Lo único que A2
agrega es *a qué símbolo aplica ahora*, recuperable del propio contenido. La
condición (b) de §7 tiene poco margen por construcción. Detalle y consecuencias
en `PROBE_PREREGISTRATION.md` §11.2.

**C — Bloquea la campaña, no el piloto.** El directorio se llama
`pilot_seed456` pero el manifest registra **seed 42**, la misma seed de la
campaña planificada. La campaña debe generarse con otra seed para que los
materiales del piloto no reaparezcan dentro de ella. El nombre del directorio
se deja como está (documentado en `PROBE_PREREGISTRATION.md` §10); renombrarlo
rompería comandos ya en circulación.

**D — Docs menores.** `CLAUDE.md:13` y `AGENTS.md:29,97` (ambos untracked)
afirman que no hay directorio `tests/`; sí lo hay
(`tests/experiment_integration.rs`, `tests/memory_lifecycle.rs`).

---

## 5. El resultado del piloto 2

| Brazo | Todas | Drift | fix_pass | Falsa confianza |
|---|---|---|---|---|
| A0 | 1/6 | 1/4 | 6/6 | 0 |
| A1 | 0/6 | 0/4 | 6/6 | 0 |
| A2 | 0/6 | 0/4 | 6/6 | 0 |

Contra `go-no-go.md` §7: la condición (a) **se cumple** (A2 0/6 estrictamente
por debajo de A0 1/6). La (b) **no**: A2 y A1 empatan 0/4 en drift, y §7 hace de
eso un disyunto de NO-GO.

La ampliación de zona gris **no se tomó**, por el pre-compromiso §12.5 escrito
antes de ver los números: A1 violó 0/4 en drift, así que el empate es B2
confirmado — materiales, no potencia — y §0 manda arreglar el diseño, no comprar
más n. **Veredicto: NO-GO.**

La predicción §12.4 se confirmó entera, mecanismo incluido. Todo el registro está
en `supplementary/pilot_seed456/PROBE_PREREGISTRATION.md` §12 y §13, escrito por
etapas y siempre antes de la etapa siguiente.

---

## 6. El hallazgo que vale más que el veredicto

En repo_03 (drift de rename) A2 llamó `recall` sobre `src/billing.rs`,
`issue_number`, `NEXT_INVOICE` y `format_invoice`. **Los cuatro devolvieron
`count: 0`.** Nunca probó `render_invoice`, el nombre muerto, y no tenía cómo
saberlo. La memoria huérfana jamás se recuperó, así que el clasificador de
frescura nunca corrió sobre ella.

La causa era del producto: `scope_matches` comparaba por igualdad exacta de
string, y `classify` sabía resolver el sucesor pero ese resultado solo se usaba
para **anotar**, nunca para **encontrar**. Dicho corto: el retrieval funcionaba
exactamente cuando el nombre del ancla no había cambiado, que es exactamente
cuando la frescura no aporta nada.

**Ya está arreglado** (`goals/RECALL_RETRIEVAL_GOAL.md`): búsqueda por candidato
de re-anclaje limitada a las bases `SigHash`/`ShapeHash`, retrieval por archivo,
y `file` relativo a la raíz del índice en `AnchorKey` con `#[serde(default)]`.
Verificado end-to-end contra el repo_03 real: las tres consultas que devolvían
0 ahora devuelven la memoria, marcada con `reached_via` y con status `orphaned`.

Nota de honestidad, obligatoria en cualquier piloto 3: el arreglo toca el MCP
base, que `go-no-go.md` decía no tocar, y le da a A2 una capacidad que un
`gotchas.md` no tiene. Se justifica porque es un bug del sistema bajo prueba
descubierto por el experimento y documentado antes de decidir arreglarlo — pero
el sistema bajo prueba cambió entre pilotos y eso no se puede comparar de
frente. El argumento completo está en el goal.

---

## 6b. La bifurcación abierta

- **Cerrar con el negativo.** Informe del piloto 2: NO-GO documentado, dos
  defectos de instrumento con evidencia, metodología limpia. §7 dice
  explícitamente que un resultado negativo es capítulo válido.
- **Arreglar y re-pilotear.** Requiere el retrieval (hecho) más los materiales
  según `goals/MEMORY_MISDIRECTION_GOAL.md`, más una campaña con seed ≠ 42.

---

## 7. Archivos clave

| Archivo | Qué contiene |
|---------|--------------|
| `go-no-go.md` | Protocolo **sellado** (v2): umbrales, diseño, rúbrica FC, cronograma |
| `supplementary/pilot_seed456/PROBE_PREREGISTRATION.md` | **Protocolo vigente** del probe A0 + enmienda §11 |
| `supplementary/pilot_seed123/PILOT_REPORT.md` | Informe del piloto 1: 18 corridas, hallazgos, veredicto |
| `doc-estado-y-tesis.md` | Diagnóstico del grafo, pivote a memoria, marco teórico, framing |
| `RESUMEN_EJECUTIVO.md` | Mapa condensado (desactualizado: dice 184 tests) |
| `goals/GENERATOR_REDESIGN_GOAL.md` | Spec de los 6 fixes (implementados) |
| `goals/SYNTH_GENERATOR_GOAL.md` | Spec original de la infra experimental |
| `supplementary/rubric.md` | Rúbrica de falsa confianza (con ventana de verificación) |
| `tools/synth_repo_gen/scenarios.rs` | Catálogo de invariantes, drifts, `fix_wrong` |
| `tools/experiment_runner/capture.rs` | Prompts de siembra A1/A2 (origen del hallazgo B2) |
| `tools/experiment_runner/plan.rs` | `run_id` y resume |
| `tools/scorer/stats.rs` | Ventana de verificación FC |

---

## 8. Reglas que no se tocan

1. **`go-no-go.md` está sellado.** Ni umbrales ni diseño experimental.
2. **No tocar parser / grafo / MCP base.** Ya funcionan.
3. **El piloto §0 es compuerta obligatoria** antes de cualquier campaña.
4. Los problemas de materiales se arreglan cambiando **el diseño, no el
   protocolo** (go-no-go §0 lo autoriza explícitamente).
5. Toda enmienda al pre-registro se escribe **antes** de ver números, fechada y
   en su propio commit. Una edición silenciosa a un pre-registro es peor que no
   tenerlo.
6. **El veredicto final no determina si la tesis pasa.** Un NO-GO bien
   documentado es un capítulo válido; lo que importa es la metodología honesta.
