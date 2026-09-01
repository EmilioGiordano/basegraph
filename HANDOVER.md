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

## 5. Dónde estamos: el probe A0 es la compuerta

Por el hallazgo B decidimos no gastar las 18 corridas de una vez.
`supplementary/pilot_seed456/PROBE_PREREGISTRATION.md` es **el protocolo
vigente**: brazo A0 solo, 6 tareas, 1 seed, sin sesiones de siembra, con regla
de parada firmada antes de correr y enmendada (§11) también antes de correr.

Regla de parada vigente, sobre el **subconjunto drift** (repo_02 + repo_03,
4 de 6 tareas):

| Resultado A0 | Decisión |
|---|---|
| drift ≥1 | **SEGUIR** con siembra + A1/A2 (12 corridas), reutilizando las 6 de A0 |
| drift 0, total ≥1 | Condición (b) no testeable. **No lanzar las 12.** Spike de 2 corridas A1 en repo_03 (~$2) para probar el mecanismo de descarte; seguir solo si A1 viola al menos una vez |
| total 0, `fix_pass` ≥5/6 | **PARAR.** Arreglar materiales y re-pilotear |
| total 0, `fix_pass` <5/6 | **PARAR**, pero el diagnóstico es dificultad/caps, no localidad |

Comandos exactos, precondiciones de aislamiento (`CLAUDE_CONFIG_DIR`,
`--work-dir` fuera del árbol con `CLAUDE.md`) y cómo leer el resultado: §8 y §9
del pre-registro. El veredicto §7 que imprime el scorer **no** es la salida del
probe (con un solo brazo siempre da GREY); la salida es la línea
`a0: n/6 = … drift n/4` más la columna `fix_pass`.

---

## 6. Después del probe

- **Si SEGUIR:** sembrar A1/A2 y correr las 12 restantes sobre los mismos
  materiales, con `--seeds 1` (el `run_id` termina en `-s0`; otro valor
  re-ejecuta y re-paga las 6 de A0). Antes: `cargo build --release` completo
  (hallazgo A).
- **Si PARAR:** los fixes candidatos ya están listados en el pre-registro §6 y
  §11.2, escritos antes de ver números — test de regresión de C2 bajo `tests/`,
  mensajes de C2 no autodiagnósticos, cambiar el escenario de repo_03, y hacer
  que la memoria stale sea *engañosa* y no solo mal direccionada.
- **Campaña completa** (solo con señal): 10 repos, seed ≠ 42, `--verify`,
  runner, scorer → `verdict.txt`.

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
