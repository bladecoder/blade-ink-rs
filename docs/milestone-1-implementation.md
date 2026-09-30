# Milestone 1: qué ha cambiado en el runtime

## La idea en pocas palabras

Una historia Ink contiene instrucciones que no cambian durante la partida (texto, contenedores, desvíos, nombres de rutas y definiciones de listas) y datos que sí cambian (variables, posición actual, pila de llamadas, elecciones y contadores).

Antes, el runtime representaba la parte fija como un árbol de objetos enlazados mediante muchos `Rc`. Algunos de esos objetos también guardaban cachés mutables. Ahora, el constructor público `Story` guarda las instrucciones en un **arena plano e inmutable** y deja los datos cambiantes en una estructura de ejecución separada. `LegacyStory` conserva el intérprete anterior para poder comparar los resultados durante la migración.

```text
JSON compilado
    │
    ▼
Arena de contenido: nodos numerados, hijos, nombres y listas  ← no cambia
    │                           ▲
    │ consulta por ID           │ destinos ya enlazados
    ▼                           │
Estado de ejecución: posición, variables, pila, flujos, elecciones y contadores
```

En este milestone el arena **sigue estando en RAM** y el JSON **se sigue leyendo al crear** `Story`. La imagen binaria que podrá consultarse desde flash se hará en los milestones 2 y 3.

## Cómo elegir el intérprete nuevo o el anterior

Los dos intérpretes siguen disponibles en esta rama. La elección se hace con el **tipo que se construye**, no con un ajuste que cambie una historia ya creada:

```rust
use bladeink::story::{LegacyStory, Story};

let mut nuevo = Story::new_with_seed(json, 42)?;
let mut anterior = LegacyStory::new_with_seed(json, 42)?;
```

`Story` es el nombre público habitual y ahora es un alias de `FlatStory`, la implementación del arena plano. Por tanto, el código existente que llama a `Story::new(...)`, `Story::new_from_reader(...)` o sus variantes con semilla selecciona el intérprete **nuevo** sin cambiar sus llamadas. `LegacyStory` conserva el árbol y el intérprete anteriores para comparar comportamientos, medir costes y detectar regresiones. La herramienta `rinklecate` importa `Story`, así que también usa el intérprete nuevo al reproducir una historia.

La característica de Cargo `stream-json-parser` elige el **lector JSON**, independientemente del intérprete. Puede usarse con `Story` o con `LegacyStory`; no es un selector «nuevo/legacy». Ambos reciben el mismo JSON compilado y pueden intercambiar estados guardados en el formato Ink. Para una comparación reproducible conviene usar la misma semilla y la misma secuencia de elecciones, como hace el benchmark.

## Cómo se construye la historia

Cada nodo recibe un `NodeId` de 32 bits. Un contenedor tiene además un `ContainerId`; sus hijos aparecen en un rango de IDs que conserva el orden de la historia. También se guardan las relaciones de padre y los hijos con nombre, incluidos los contenedores que solo aparecen por nombre. Así se puede recorrer la jerarquía sin seguir punteros entre objetos.

El tipo de cada nodo es explícito: texto o valor, contenedor, elección, desvío, comando, etiqueta, referencia de variable, etc. Los operandos de estas instrucciones se guardan en el arena. Al terminar de leer el JSON, los destinos fijos de desvíos, elecciones y referencias de recuento se resuelven a IDs. Un destino calculado a partir de una variable se resuelve cuando se ejecuta.

Los dos lectores JSON construyen el arena directamente:

- **Serde** lee primero los tokens JSON en su representación temporal y de ahí crea los registros del arena.
- **Streaming** consume el documento progresivamente y crea esos registros sin construir el árbol antiguo ni mantener todo el JSON parseado a la vez.

Ninguno de los dos deja un árbol persistente de `Rc<dyn RTObject>` dentro de `Story`. El camino que convierte un árbol antiguo en un arena permanece como apoyo para las pruebas comparativas; el constructor público no lo usa.

## Cómo se ejecuta

La posición del intérprete es básicamente «contenedor N, hijo i». Para pasar a la instrucción siguiente consulta el rango de hijos; para entrar en un contenedor o seguir un desvío usa IDs. En la ejecución habitual no necesita generar la ruta Ink como texto ni buscar un nodo por identidad de puntero.

El estado mutable contiene la pila de llamadas y sus hilos, variables globales y temporales, pila de evaluación, salida de texto y etiquetas, elecciones generadas, flujos, semilla aleatoria y recuentos de visitas y turnos. Los contadores usan `ContainerId` como clave. Los valores que se calculan al ejecutar una instrucción pueden seguir usando `Rc` donde lo necesita el estado; los nodos **estáticos** del arena no contienen `Rc`, `RefCell` ni `OnceCell`.

Se ha migrado la ejecución de texto y glue, expresiones, condiciones, elecciones, desvíos, túneles, funciones Ink, listas, etiquetas, secuencias y números aleatorios. También funcionan las llamadas externas, las variables observadas, los flujos y `continue_async`, que puede pausar y reanudar entre instrucciones. La API pública `Story::new*` usa este intérprete. Al reiniciar una partida se crea un estado nuevo y se reutiliza el arena, sin volver a leer el JSON.

### Cachés lazy

Separar los datos fijos del estado **no obliga a perder todas las cachés**:

- Los destinos fijos ya están enlazados por ID, por lo que no necesitan una caché lazy durante la partida.
- Las elecciones compatibles con la API antigua (`Rc<Choice>`) se construyen la primera vez que el cliente las pide y se cachean en la instancia de `Story`. La caché se invalida cuando cambia el estado que puede cambiar esas elecciones. Si nunca se consultan, no se construyen.
- Las rutas textuales se calculan cuando una API o el guardado las necesita. Hay una caché dispersa y acotada para rutas implementada y probada, pero **aún no está conectada** al intérprete habitual; se usará si una medición muestra que las consultas repetidas lo justifican.

La caché `OnceCell` de `Path::components_string` del código antiguo se conserva. El nuevo arena no guarda esa clase de caché dentro de sus nodos. Una llamada repetida a una API que devuelve una ruta textual puede volver a construirla; el benchmark siguiente no aísla ese caso. **Podría ser más lenta esa consulta concreta**, y habría que medirla antes de conectar la caché dispersa. En cambio, los desvíos normales siguen IDs ya resueltos y no necesitan construir rutas: en el recorrido medido el intérprete nuevo fue más rápido.

## Guardar y cargar una partida

Durante la ejecución, punteros y contadores se expresan mediante IDs. El archivo de guardado mantiene el **formato JSON Ink existente**: al escribir se convierten los IDs a rutas Ink; al cargar se resuelven esas rutas otra vez a IDs. Hay códecs para Serde y para el lector por streaming.

Se han probado estados intercambiados entre `Story` y `LegacyStory`, incluidos estados con elecciones, flujos e hilos. Esto permite cambiar la representación interna sin cambiar el formato de las partidas guardadas.

## Comprobación del comportamiento

Además de la suite completa del workspace, las pruebas comparan el intérprete nuevo con el anterior en más de cien historias JSON compiladas y en una partida completa de *The Intercept*. La comparación cubre texto, etiquetas y elecciones; otras pruebas focalizadas cubren variables, listas, funciones, llamadas externas, contadores, observadores, guardado/carga y pausa asíncrona.

Pasaron las siguientes comprobaciones al cerrar el milestone:

```text
cargo test --workspace
cargo test -p bladeink --features stream-json-parser
cargo check -p bladeink --lib --no-default-features --features stream-json-parser --target thumbv7em-none-eabihf
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Tiempos y memoria medidos

El ejemplo [`flat_story_cost.rs`](../runtime/examples/flat_story_cost.rs) ejecuta *The Intercept* en un host x86_64 con compilación `release`. El JSON está incluido en el binario y no cuenta como heap. Para cada combinación se hicieron cinco pasadas; los tiempos de la tabla son valores representativos de las cuatro posteriores a la primera. El recorrido de ejecución elige siempre la opción 0 y permite hasta 20 decisiones.

«Heap retenido» son los bytes solicitados al asignador que siguen vivos justo después de crear la historia. «Pico de creación» es el máximo de esos bytes durante el constructor. La medición **no incluye** sobrecarga interna del asignador, fragmentación, pila ni el tamaño del JSON incluido en el binario.

| Lector JSON | Intérprete | Heap tras crear | Pico al crear | Crear | Ejecutar recorrido |
| --- | --- | ---: | ---: | ---: | ---: |
| Serde | Árbol anterior | 1 830 419 B | 3 819 439 B | ~3,7 ms | ~1,6 ms |
| Serde | Arena plano | 986 175 B | 3 114 107 B | ~4,1 ms | ~0,51 ms |
| Streaming | Árbol anterior | 1 868 607 B | 1 875 343 B | ~3,2 ms | ~1,5 ms |
| Streaming | Arena plano | 971 839 B | 1 115 431 B | ~4,2 ms | ~0,51 ms |

Con el lector streaming, el heap retenido baja unos **897 kB (48 %)** y el pico de creación baja unos **760 kB (41 %)** frente al árbol anterior. Con Serde, el heap retenido baja un **46 %**, pero el pico sigue cerca de **3,11 MB** por las estructuras temporales del parseo JSON. En esta máquina, construir el arena lleva algo más de tiempo; ejecutar el recorrido tarda aproximadamente un tercio de lo que tarda el intérprete anterior.

La diferencia de **construcción** se mide antes de ejecutar la historia y no puede atribuirse a la caché lazy de rutas: esa caché solo afectaría a consultas hechas durante la ejecución. El constructor nuevo crea los registros del arena y resuelve los destinos fijos. Estos resultados no separan cuánto cuesta cada paso del constructor.

Estas cifras sirven para comparar las implementaciones en el host. **No prueban que el firmware completo quepa en 2 MB de PSRAM ni en 4 MB de flash**: hay que medir en el ESP32-S3 con su asignador, el resto del firmware y la historia final. Tampoco miden el ahorro de tiempo de eliminar el parseo; ese ahorro llegará cuando la imagen binaria se genere durante la construcción y se lea directamente desde flash.

## Dónde está cada parte

| Archivo | Responsabilidad principal |
| --- | --- |
| [`flat_story.rs`](../runtime/src/flat_story.rs) | IDs, arena de nodos, enlaces, rutas y contadores por ID. |
| [`flat_json_stream.rs`](../runtime/src/json/flat_json_stream.rs) y [`json_direct.rs`](../runtime/src/flat_story/json_direct.rs) | Construcción del arena desde los dos lectores JSON. |
| [`flat_runtime.rs`](../runtime/src/flat_runtime.rs) y [`flat_callstack.rs`](../runtime/src/flat_callstack.rs) | Ejecución y pila de llamadas usando IDs. |
| [`flat_story_player.rs`](../runtime/src/flat_story_player.rs) | API pública, observadores, caché lazy de elecciones y control de `continue_async`. |
| [`state_stream.rs`](../runtime/src/flat_runtime/state_stream.rs) | Guardado y carga por streaming con el formato Ink. |

El [plan de la imagen en flash](flash-story-image-plan.md) describe los siguientes milestones. El [plan detallado del Milestone 1](milestone-1-flat-story-plan.md) conserva las decisiones técnicas y los criterios de cierre.
