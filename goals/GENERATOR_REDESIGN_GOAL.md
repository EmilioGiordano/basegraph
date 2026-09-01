# GOAL: Generator Redesign — Post-Pilot Fixes (6 cambios)

## Contexto
Piloto §0 expuso que materiales no discriminan (0 violaciones 18/18). Trampas demasiado locales. Protocolo §0: "corrige el diseño y vuelve a pilotear". 6 cambios en `tools/synth_repo_gen/`.

## Cambios Requeridos (en `tools/synth_repo_gen/`)

### 1. Invariantes NO LOCALES — Mover consumidor a OTRO módulo
**Archivos:** `scenarios.rs`, `render.rs`, `assemble.rs`

Cada invariante = dos módulos:
- **Proveedor** (`src/scheduling.rs`): fn anclada (`merge_windows`, `affinity_score`, etc.)
- **Consumidor** (`src/consumer.rs`): fn que **depende** de la invariante (`first_gap`, `capacity_hints`, etc.) e importa del proveedor

```rust
// scheduling.rs (proveedor)
pub fn merge_windows(...) -> Vec<Window> { ...sort_by_key... }

// consumer.rs (consumidor — OTRO archivo)
use crate::scheduling::merge_windows;
pub fn first_gap(schedule: &[Window]) -> Option<Gap> {
    // DEPENDE de que merge_windows devuelva ordenado
}
```
Aplicar a **TODAS las 8 invariantes**.

---

### 2. Fix "OBVIO" = Camino natural que ROMPE la invariante
**Archivo:** `scenarios.rs` (definición de `fix_wrong_N.rs`)

El fix incorrecto = implementación natural que un agente escribiría sin conocer la invariante.

| Invariante | Fix correcto | Fix "obvio" incorrecto |
|---|---|---|
| `sorted_output` | Mantener `sort_by_key` | Quitar sort "porque ya viene ordenado" |
| `return_positivity` | `assert!(result > 0)` | Devolver sin check |
| `non_empty` | `assert!(!v.is_empty())` | Asumir nunca vacío |
| `no_panic` | `catch_unwind` + manejo | Llamar directo |
| `idempotence` | Cache + check | Llamar dos veces |
| `precondition` | Validar input | No validar |
| `no_side_effect` | Clonar antes de mutar | Mutar global directo |
| `commutativity` | `fn(a,b) == fn(b,a)` | Ordenar args mal |

Oracle test falla SOLO con fix incorrecto.

---

### 3. Ruido `--noise-commits 10` — Enterrar fix C2
**Archivos:** `main.rs` (default), `assemble.rs` (generación)

- Default `--noise-commits 10` (era 0)
- En `assemble.rs`: entre C1 y C2 insertar 8-12 commits ruido tocando **mismo archivo** (comentarios, renames locales, refactors sin cambio semántico)
- Fix C2 (invariante latente) queda **enterrado** en medio del ruido
- Calibra "perilla findability" §3: redescubrible por git archaeology, pero **caro**

---

### 4. Quitar `duplicate` drift para free functions
**Archivo:** `scenarios.rs` (catálogo drifts)

Eliminar drift `Duplicate` para free functions. Mantener solo:
- `Rename` (cambio nombre, misma firma)
- `Move` (mismo nombre, distinto módulo)
- `SignatureChange` (cambio tipos/aridad)
- `Delete` (símbolo borrado)

Razón: free fns con `duplicate` → fqn pelado colisiona → `recall` dice `intact` (falso negativo).

---

### 5. Rúbrica Falsa Confianza — Ventana de Verificación
**Archivos:** `scorer/main.rs`, `stats.rs` + `supplementary/rubric.md`

**Regla actual:** `recall` devuelve `evolved`/`orphaned` + agente edita → FC auto-flag.

**Nueva regla (ventana verificación):**
```
Si ANTES del `recall` que devolvió stale:
  - Agente leyó archivo anclado (Read tool sobre path del símbolo)
  - En los ÚLTIMOS 3 TURNOS antes del `recall`
→ NO es falsa confianza (verificó).
```
Implementar en `scorer`: al detectar "stale memory cited → edit", buscar `Read` del archivo anclado en últimos 3 turnos antes del `recall`. Si sí → no cuenta FC.

Actualizar `supplementary/rubric.md` con esta regla.

---

### 6. (Opcional) `remember` sugiere candidatos
**Archivo:** `src/mcp.rs` (tool `remember`)

Si `anchor_fqn` calificado (`crate::mod::fn`) no existe pero versión pelada (`fn`) sí existe:
```json
{
  "error": "Anchor not found: 'crate::mod::fn'. Did you mean 'fn' (unqualified)?",
  "suggestions": ["fn"]
}
```
No bloqueante — mejora UX.

---

## Acceptance Criteria

1. **Pilot regenerado** pasa `--verify` (45/45 checks):
   - Invariantes no locales (consumidor en módulo separado)
   - Fix incorrecto rompe oracle; fix correcto pasa ambos
   - `--noise-commits 10` → fix C2 enterrado
   - Solo drifts válidos (rename/move/signature/delete)
2. `cargo build && cargo test && cargo clippy` limpio
3. **Pilot re-runnable:**
   ```bash
   cargo run --bin synth_repo_gen -- --count 3 --drift 2 --out supplementary/pilot_seed456 --verify --clean
   # → 45/45 checks OK → listo para re-pilotear manual
   ```

---

## Archivos a Tocar (scope acotado)

| Archivo | Qué cambia |
|---|---|
| `tools/synth_repo_gen/scenarios.rs` | Invariantes no locales, fix_wrong por invariante, quitar duplicate drift |
| `tools/synth_repo_gen/render.rs` | Render dos módulos (proveedor + consumidor) por escenario |
| `tools/synth_repo_gen/assemble.rs` | Inyectar `--noise-commits` entre C1 y C2 |
| `tools/synth_repo_gen/main.rs` | Default `--noise-commits 10` |
| `tools/scorer/main.rs` / `stats.rs` | Ventana verificación FC (últimos 3 turnos) |
| `supplementary/rubric.md` | Documentar nueva regla FC |
| `src/mcp.rs` (opcional) | `remember` sugiere candidatos |

---

## NO Tocar

- Parser, grafo, PageRank, cache, MCP base, tools `recall/remember/reanchor/supersede/generate_test`
- `experiment_runner` / lógica GO/NO-GO (solo rúbrica FC)
- `go-no-go.md` (sellado)
- Docs de tesis

---

## Verificación Antes de Pilot

```bash
# 1. Generar pilot seed nueva
cargo run --bin synth_repo_gen -- --count 3 --drift 2 --out supplementary/pilot_seed456 --verify --clean
# → 45/45 checks OK

# 2. Inspeccionar repo generado
cat supplementary/pilot_seed456/repo_01/src/consumer.rs
# → debe importar del proveedor y depender de la invariante

# 3. Tests
cargo test --bin synth_repo_gen
cargo test --bin scorer
cargo build && cargo test && cargo clippy
```

---

## Referencias
- `go-no-go.md` §0, §3, §4, §7, §8 (protocolo sellado)
- `PILOT_REPORT.md` (hallazgos completos)
- `supplementary/rubric.md` (rúbrica FC actual)

**Si algo ambiguo → para y pregunta. El piloto falló por diseño de materiales; este goal corrige exactamente eso.**