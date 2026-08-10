# Go/No-Go — Memoria anclada para agentes (protocolo pre-registrado, v2)

Experimento decisivo de Fase D. Decide si existe Fase E (captura automática,
memoria→test, confirmación de re-anclajes) o si el pivote se cierra con un
resultado negativo documentado. **Este documento se sella antes de correr
ningún dato; los umbrales de la sección 8 no se tocan después.**

v2: integra las enmiendas de alcance, coaching simétrico, potencia al eje
decisivo y piloto manual previo, acordadas antes del sellado. El commit de esta
versión es el sello de pre-registro.

## 0. Gate previo: piloto manual (obligatorio)

Antes de construir ningún harness automatizado: **2-3 repos × 2 tareas, corridas
a mano**, sin automatizar nada. El harness para 60 corridas autónomas con
scoring es, de hecho, un mini-SWE-bench — más trabajo que el sistema testeado,
y no se construye hasta saber que el diseño sirve.

El piloto es compuerta: solo se escala a los 10 repos si (a) muestra señal
detectable en alguna dirección y (b) confirma que los repos e invariantes se
comportan como se diseñaron (la invariante es latente, el fix "obvio" la viola,
el test la detecta). Si el piloto expone problemas de diseño, se corrige el
diseño — no el protocolo — y se vuelve a pilotear.

El generador de repos se construye pensando el primer batch (2-3 repos) como
material del piloto.

## 1. Preguntas de investigación y alcance del claim

- **RQ1 (valor absoluto):** ¿Un agente con memoria anclada viola menos las
  invariantes ocultas de un codebase que un agente sin memoria?
- **RQ2 (valor relativo, el decisivo):** ¿El sistema anclado le gana a un
  `gotchas.md` curado cuando el código se movió desde que se escribió la
  memoria (drift)?
- **RQ3 (costos):** ¿Qué cuesta en tokens/tiempo? ¿Cuánto daño hace una memoria
  stale servida con autoridad (falsa confianza)?

**Alcance declarado del claim.** A esta escala (20-60 archivos) el `gotchas.md`
entra entero en contexto: A1 está artificialmente fuerte y su debilidad real
(no escala / no se scopea) *no se testea*. Además A2 paga un impuesto de
retrieval. Consecuencia: **este experimento decide "frescura bajo drift le gana
al .md", NO "le gana a escala".** Un GO licencia la tesis de frescura; la de
escala queda como validación futura independiente. Un NO-GO en el eje no-drift
es, por diseño, ininterpretable — y se reporta como tal, no como fracaso.

## 2. Diseño

Tres brazos, mismo agente (mismo modelo, versión pineada, temperatura fija),
misma tarea, instancia fresca por corrida:

| Brazo | Herramientas | Coaching | Qué testea |
|---|---|---|---|
| **A0 — baseline** | read/grep/glob + `git log` completo | Ninguno extra | Redescubrimiento lazy (la alternativa gratuita) |
| **A1 — markdown** | A0 + `gotchas.md` curado | "Hay un `gotchas.md`, consultalo para lo que vas a tocar" | El competidor degenerado |
| **A2 — anclado** | A0 + MCP `recall`/`remember` (sin el .md) | "Hay un tool de memoria, consultalo" | El sistema |

**Coaching simétrico (decisión fijada).** Sin coaching, A2 podría no usar
`recall` y el experimento mediría *discoverability*, no memoria. La resolución
justa es simétrica: A1 y A2 reciben la misma instrucción de consulta (que es el
uso real desplegado de ambas herramientas); A0 no recibe nada extra. Así se
mide **calidad + frescura**, no quién descubre la herramienta. Igualmente se
registra si la herramienta fue efectivamente consultada en cada corrida.

El baseline tiene git **a propósito**: si A0 redescubre la invariante haciendo
arqueología de commits, se mide cuánto le cuesta. Es la misma objeción lazy que
mató al grafo; no nos la autoinfligimos.

## 3. Materiales: repos sintéticos con historia scripteada

Generar **10 repos** Rust (codegraph es Rust-only), 20-60 archivos cada uno,
con historia de git scripteada:

1. Commit base: el codebase funcionando.
2. Commit de bug + fix: se introduce y arregla un bug cuyo fix deja una
   **invariante latente**. El commit message menciona el fix *sin* gritar la
   invariante (nada de `WARNING: never remove X` — eso sería regalar el juego).
3. **En 7 de los 10 repos (condición drift):** un commit posterior de refactor
   que renombra/mueve el símbolo anclado, sin tocar la semántica. (La
   comparación decisiva, A2 vs A1 en drift, merece la mayor potencia: 14
   tareas, no 10.)

**Oráculo binario determinista, obligatorio.** Toda invariante debe ser
chequeable por un test determinista pre-escrito. Invariantes sin oráculo
determinista se podan en diseño: "evita la race del issue #N" queda **fuera**;
valen orden load-bearing, precondiciones no obvias, cache-key-debe-incluir-X,
"esto parece bug pero es a propósito" verificable.

**Perilla de findability por git.** La invariante debe ser redescubrible por
arqueología de commits pero **cara y enterrada**: ni trivial (A0 gana gratis)
ni imposible (injusto con el baseline). Es una perilla del generador: mensajes
de commit realistas pero no explícitos, fix enterrado en una historia con
ruido. El piloto (§0) calibra esta perilla antes de escalar.

Cada repo define **2 tareas** (20 corridas por brazo, 60 totales): un bug report
o feature request redactado en seco, cuyo fix correcto **no** viola la
invariante latente, pero cuyo fix "obvio" sí.

**Ground truth pre-escrita:** por tarea, antes de cualquier corrida: (a) el fix
correcto de referencia, (b) el test determinista que detecta la violación,
(c) la suite que valida el fix primario.

## 4. Siembra de memorias — por el pipeline real, no a mano

Por repo, se corre una **sesión previa** con un agente que resuelve el bug del
paso 2 y luego registra lo aprendido por el proceso de captura previsto:

- Para A1: el agente destila un `gotchas.md` (calidad realista, no oráculo).
- Para A2: el agente escribe vía `remember` (ancla, kind, contenido, commit).

Prohibido editar las memorias a mano después. Si el capturador produce basura
y el experimento falla, **ese es el hallazgo** — el write path es el riesgo
mayor del producto y medirlo es parte del experimento. Se guardan las memorias
crudas generadas y se reporta cuántas por repo resultaron utilizables.

## 5. Procedimiento por corrida

1. Instancia fresca del agente, herramientas y coaching según brazo, prompt de
   tarea pelado.
2. Cap de tokens y tiempo (mismo para los tres brazos); si se agota, la corrida
   cuenta como fallo de la métrica primaria si no hay fix válido. El cap actúa
   además como cota de sanidad del costo (ver §7).
3. Se ejecuta la suite del fix primario y el test de la invariante oculta.
4. Se registra: violación (sí/no), fix primario (pasa/no pasa), tokens, tiempo,
   llamadas a herramientas, y — por brazo — si consultó la memoria (A2), si leyó
   el .md (A1), si hizo arqueología de git (A0).
5. Orden de tareas y repos aleatorizado entre corridas.
6. **Varianza del agente:** aun a temperatura 0 los caminos de tool-use
   divergen. Se corren **2 seeds por tarea** como análisis de sensibilidad.
   Ojo: las corridas repetidas de una misma tarea NO son muestras
   independientes (misma tarea, mismo repo) — se reportan como sensibilidad,
   no inflan el n estadístico.

## 6. Métricas

- **Primaria (binaria):** violación de la invariante oculta (test pre-escrito).
- **Secundarias:** corrección del fix primario; tokens; tiempo; llamadas.
  El **costo se mide solo sobre la corrida de tarea** — la sesión de siembra no
  cuenta (es costo de instalación, no de uso).
- **De daño (condición drift):** tasa de corridas donde el agente cita una
  memoria stale como vigente y actúa en consecuencia (falsa confianza).
  Valor neto del sistema = aciertos − engaños; sin el segundo término el
  experimento es promocional, no decisivo.

## 7. Umbrales de decisión (pre-comprometidos)

Con n=20 por brazo (14 en drift), proporciones sobre violación de invariante.
**Dos condiciones primarias** (evitamos el AND de cuatro, que con este n cae en
zona gris por ruido):

- **GO:** (a) A2 viola **estrictamente menos** que A0 sobre las 20 tareas, **y**
  (b) en la condición drift A2 viola **estrictamente menos** que A1 con tasa de
  falsa confianza ≤ A1.
- **NO-GO:** A2 no viola menos que A0, **o** en drift A2 no le gana a A1 (el
  sistema no supera a un .md → no hay producto), **o** la falsa confianza de A2
  es materialmente peor que la de A1.
- **Cota de sanidad (fuera de las compuertas):** si A2 agota el cap de tokens
  sistemáticamente donde A0/A1 no, se reporta y pesa en la interpretación
  aunque no sea compuerta formal.
- **Secundarias reportadas, no compuertas:** costo en tokens, tiempo, RQ1 en
  términos absolutos.
- **Zona gris** (empates o diferencias de una corrida): se permite **una**
  ampliación a 3 tareas por repo (n=30, 21 en drift) con los mismos umbrales.
  Si sigue gris, es NO-GO.

Resultado negativo no es fracaso del trabajo: es el capítulo empírico de la
tesis, con el mismo estatus que el resultado negativo del grafo.

## 8. Controles de validez

- **Contaminación:** repos sintéticos generados para este experimento,
  inéditos; el modelo no pudo verlos en entrenamiento.
- **Sesgo del evaluador:** quien puntúa "falsa confianza" usa una rúbrica
  escrita acá (actuar sobre una memoria cuyo ancla está `evolved`/`orphaned`
  sin verificar contra el código actual = engaño), no juicio libre.
- **Varianza de captura:** las memorias las escribe el mismo modelo que corre
  las tareas, en sesión separada; se reporta cuántas memorias por repo resultaron
  utilizables.
- **n chico, admitido:** n=20/brazo da potencia limitada; se reportan
  proporciones con intervalos de confianza, no p-values heroicos. Los umbrales
  están elegidos para ser legibles con ese n.
- **Generalización:** repos sintéticos ≠ repos reales; y la escala (§1) queda
  explícitamente fuera del claim. Son los precios de la ground truth perfecta;
  se declaran como limitaciones, no se esconden.

## 9. Ejecución y cronograma estimado

0. **Piloto manual (§0):** batch de 2-3 repos, corridas a mano (2-3 días).
   Compuerta: ¿señal detectable y materiales sanos?
1. Generador completo + 10 repos + historia scripteada (2-3 días).
2. Sesiones de captura A1/A2 (1-2 días).
3. 60 corridas × 2 seeds + scoring (3-4 días, automatizable salvo falsa
   confianza; solo si el piloto pasó).
4. Análisis y veredicto contra sección 7 (1 día).

Total: ~2-3 semanas, con punto de salida barato en el paso 0. Todo artefacto
(repos, memorias crudas, logs de corridas, rúbrica) queda versionado como
material suplementario de la tesis.

## 10. Lo que este experimento NO decide

La captura automática a escala, la síntesis memoria→test, multi-lenguaje, la
confirmación de re-anclajes y **la tesis de escala** (§1) quedan fuera: las
primeras son Fase E y solo existen si hay GO; la última es validación futura
independiente.
