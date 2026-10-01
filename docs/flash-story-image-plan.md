# Historias Ink ejecutadas desde una imagen en flash

## Objetivo y alcance

Generar durante la construcción una imagen binaria propia de `bladeink` a partir de una historia `.ink` o `.ink.json`. El runtime debe consultar el contenido narrativo directamente desde los bytes embebidos en la flash de un ESP32-S3 con 4 MB de flash y 2 MB de PSRAM. Al crear la historia no debe parsear el JSON ni reconstruir en RAM el árbol de contenido.

La entrega comprende el runtime, el generador de escritorio, las pruebas y la documentación de integración. La sustitución de InkCpp en `ink-tts-esp32`, la compilación del firmware Rust y las medidas físicas en la placa quedan para un trabajo posterior. El nuevo formato se inspira en el uso de secciones y offsets de InkCpp, pero no será compatible con sus archivos `.bin`.

## Diseño acordado

- Separar los datos inmutables de la historia del estado de ejecución. Los nodos, sus relaciones, nombres, texto, rutas y definiciones de listas serán inmutables. Variables, elecciones generadas, pilas, flujos, contadores y valores calculados seguirán en RAM.
- Dar a cada nodo un identificador estable de 32 bits. El intérprete accederá al contenido mediante una interfaz común: una representación propia en RAM para los constructores JSON existentes y una vista de solo lectura de la imagen para el nuevo constructor. La ejecución de ambos backends compartirá la misma lógica.
- Eliminar del contenido estático las cachés y mutaciones actuales de padre, ruta y destino. Resolver durante la generación los destinos estáticos; los destinos que dependen de variables se resolverán en tiempo de ejecución. Conservar las rutas Ink canónicas necesarias para las API y los estados guardados.
- Definir una imagen determinista y versionada, con enteros little endian, cabecera con versión de formato e Ink, longitud y secciones con offsets relativos al inicio. Las secciones contendrán nodos, relaciones y contenido con nombre, cadenas y listas. No incluir punteros absolutos ni estructuras Rust serializadas directamente.
- Validar una vez al abrir la imagen las versiones, longitudes, límites, identificadores y cadenas UTF-8. Después, leer sus secciones sin copiarlas a un grafo en RAM ni modificar los bytes recibidos. Una imagen inválida devolverá un error, sin lecturas fuera de rango.
- Mantener las API actuales de carga desde JSON, juego y guardado/carga del estado. Añadir la característica `binary-image` para `no_std + alloc`; una compilación con solo esa característica no debe incorporar el parser de historias JSON, aunque sí el código necesario para los estados JSON.

### Interfaces nuevas

| Interfaz | Comportamiento |
| --- | --- |
| `Story::new_from_image_with_seed(&'static [u8], i32)` | Construye una historia desde bytes embebidos sin copiar el contenido; disponible con `binary-image` y `no_std`. |
| `Story::new_from_image(&'static [u8])` | Variante con semilla generada por el sistema; disponible con `binary-image` y `std`. |
| `bladeink::image::compile_json_to_image(reader)` | Convierte JSON compilado en `Vec<u8>` en el host; disponible con `binary-image` y `std`. |
| `rinklecate --image -o salida.inkb entrada.ink` | Compila Ink y genera la imagen, sin escribir un JSON intermedio. |
| `rinklecate --image -o salida.inkb entrada.ink.json` | Convierte el JSON compilado directamente. |

`--image` será un modo de salida y rechazará su combinación con los modos de reproducción (`-p`) y estadísticas (`-s`). Los constructores JSON y el comportamiento predeterminado de `rinklecate` conservarán sus interfaces.

## Plan de implementación incremental

### Milestone 0 — Preparación

Crear `feat/flash-story-image` desde el `HEAD` limpio de `feat/non_std` y guardar este documento en `docs/flash-story-image-plan.md`.

**Criterio de cierre:** la nueva rama está activa y el documento figura como único cambio previsto de esta preparación.

### Milestone 1 — Datos inmutables e intérprete común

Sustituir el árbol persistente de `Rc<dyn RTObject>` por un arena plano de nodos identificados por ID. Adaptar los parsers JSON y migrar navegación, ejecución y estado a accesos por ID. Separar de los nodos estáticos las cachés y los valores que cambian durante la partida. El diseño, las etapas y el tratamiento de las cachés lazy están detallados en [milestone-1-flat-story-plan.md](milestone-1-flat-story-plan.md).

**Criterio de cierre:** las API JSON existentes, sus pruebas y la suite de conformidad mantienen el comportamiento anterior. El contenido cargado es inmutable después de la construcción.

**Estado:** completado en `feat/flash-story-image`. `Story` usa el arena plano y el intérprete por IDs. Ambos parsers JSON construyen el contenido sin árbol persistente; variables, pila, flujos, contadores y cachés lazy pertenecen a la instancia de ejecución. El estado guardado conserva el formato Ink y se intercambia con `LegacyStory`, que permanece como referencia de comparación. La suite del workspace, el parser streaming, `no_std` en un objetivo embebido, formato y Clippy estricto pasan. Las medidas de heap del host están en [milestone-1-flat-story-plan.md](milestone-1-flat-story-plan.md). La lectura directa de bytes en flash corresponde al milestone 3.

### Milestone 2 — Codificador y formato

Implementar el codificador de la representación inmutable y su formato versionado. Resolver y almacenar referencias estáticas, y añadir índices para contenido con nombre y rutas usadas por navegación y estados guardados. Exponer `compile_json_to_image` y añadir `rinklecate --image` para `.ink` y `.ink.json`.

**Criterio de cierre:** la misma entrada produce bytes idénticos en ejecuciones repetidas; el generador rechaza entradas inválidas y los archivos resultantes incluyen todos los tipos de contenido cubiertos por el runtime.

**Estado:** implementado en `binary-image`. El formato v2, que añade CRC-32 para detectar corrupción, está documentado en [flash-story-image-format.md](flash-story-image-format.md). `compile_json_to_image` codifica el arena después de normalizar el orden de IDs de contenido con nombre; `rinklecate --image` acepta `.ink` y `.ink.json`. Las pruebas cubren todas las variantes de nodo, determinismo, entradas inválidas y el CLI. Los 121 JSON del corpus de conformidad se codificaron con ambos lectores y produjeron imágenes idénticas.

### Milestone 3 — Vista binaria desde flash

Implementar la validación y lectura por offsets desde `&'static [u8]`. Añadir los constructores de imagen y la configuración `binary-image` sin parser de historias JSON. Mantener el codec del estado JSON y evitar asignaciones persistentes por nodo al crear o ejecutar una historia desde la imagen.

**Criterio de cierre:** una historia embebida con `include_bytes!` se ejecuta en host con `binary-image` y la biblioteca compila en un objetivo `no_std` real. Las imágenes truncadas, corruptas o de versión incompatible se rechazan sin fallos de memoria.

**Estado:** implementado. `Story::new_from_image_with_seed` valida la cabecera, el CRC-32, las secciones y las referencias, y ejecuta desde una vista prestada de la imagen. `binary-image` funciona sin parser de historias JSON y conserva el guardado y la carga del estado JSON. La prueba con `include_bytes!` ejecuta y restaura una historia; las pruebas rechazan imágenes truncadas, corruptas y de versión incompatible. El runtime compila para `thumbv7em-none-eabihf` con solo `binary-image`. La equivalencia exhaustiva con JSON corresponde al Milestone 4.

### Milestone 4 — Equivalencia funcional

Ejecutar historias JSON e imagen con la misma semilla y secuencia de elecciones. Comparar texto, elecciones, etiquetas, variables, listas, funciones, flujos, rutas y guardado/restauración del estado. Incluir una partida completa de *The Intercept*, además de casos focalizados de referencias relativas, desvíos variables y contenido dinámico.

**Criterio de cierre:** ambos backends producen los mismos resultados observables y los estados guardados pueden cargarse con la misma historia en el otro backend.

### Milestone 5 — Memoria, documentación y cierre

Generar imágenes para las historias inglesa y española de `ink-tts-esp32`. Registrar tamaño de cada imagen y asignaciones al crear y ejecutar la historia en host, separando el estado mutable del contenido estático. Documentar el paso de generación durante la construcción y el embebido de bytes en flash.

**Criterio de cierre:** los bytes de la historia permanecen en la imagen; la memoria persistente de `Story` al abrirla no crece con el número de nodos. Publicar las medidas observadas sin presentarlas como mediciones físicas del ESP32-S3.

## Validación obligatoria

Tras cada milestone, ejecutar `cargo fmt --all` y `cargo clippy --workspace --all-targets --all-features -- -D warnings`, de acuerdo con `AGENTS.md`. Al cerrar la implementación, ejecutar también `cargo test`, `cargo test -p bladeink --features stream-json-parser`, la suite de conformidad con el backend de imagen y una comprobación `no_std` del runtime con `binary-image`. Documentar el tamaño de las imágenes y los resultados de memoria; no asumir que una compilación de host demuestra que el firmware completo cabe en 4 MB de flash.
