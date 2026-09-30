# Milestone 1: contenido plano y estado de ejecución separado

## Decisión

Sustituir el árbol persistente de `Rc<dyn RTObject>` por una representación plana de contenido estático. La jerarquía de Ink se conserva mediante IDs, rangos de hijos y relaciones de padre. El intérprete no debe depender de identidad de punteros ni escribir en los nodos de la historia. El parser JSON seguirá disponible; la imagen binaria en flash y su codificador pertenecen a los milestones 2 y 3.

Los cambios anteriores que añadían `NodeId` a los objetos del árbol eran un prototipo de transición. Se descartan como base del intérprete nuevo: conservar el árbol junto a un índice duplicaría referencias y RAM sin resolver la separación entre datos estáticos y estado.

## Modelo de contenido

- `NodeId` y `ContainerId` son IDs compactos, locales a una historia. No son punteros ni direcciones de memoria.
- `OwnedStoryData` contiene arenas de nodos, hijos ordenados, contenido con nombre, cadenas y definiciones de listas. El contenido tiene un tipo explícito (`NodeKind`), sin `dyn RTObject` por nodo.
- Cada contenedor expone un rango de IDs de hijos. Las entradas de contenido con nombre enlazan nombre e ID; los contenedores solo presentes en ese índice también reciben un ID. Los enlaces de padre y el índice del hijo permiten reconstruir rutas sin buscar por identidad de puntero.
- El destino de un desvío, una elección o una referencia de recuento estático se resuelve a un ID durante la finalización de los datos. Los destinos dependientes de variables se resuelven durante la ejecución.
- Las cadenas y rutas persistentes son datos inmutables. El modelo no incluye `Rc`, `Weak`, `RefCell` ni `OnceCell` en los nodos estáticos. La futura vista binaria debe poder ofrecer los mismos accesos sin reconstruir objetos en RAM.
- Una interfaz de lectura de datos separa el intérprete del almacenamiento concreto. El backend JSON poseerá el arena; el backend de imagen leerá offsets y slices. La interfaz debe evitar un objeto virtual o una asignación por nodo.

## Estado y cachés lazy

- `StoryState` guarda un cursor `(ContainerId, índice)` y valores dinámicos propios. Variables, elecciones, pilas, flujos, salida, contadores y contenedores temporales pertenecen al estado. Las referencias a contenido estático usan IDs; los valores dinámicos no se insertan en el arena estático.
- Los contadores de visitas y turnos, también durante los parches de estado, se indexan por `ContainerId`. Las rutas Ink se generan o resuelven únicamente en los límites de la API y del JSON de guardado, que conserva su formato.
- Los destinos estáticos preenlazados eliminan las cachés de `Divert` y `ChoicePoint` durante la partida. No se generan cadenas de ruta para el control de flujo habitual.
- Si una ruta textual u otro resultado derivado se solicita repetidamente, una caché dispersa perteneciente a la instancia de ejecución puede guardarlo por `NodeId`. La caché se asigna al primer uso, contiene solo nodos consultados, tiene un límite configurable y se puede descartar sin afectar al comportamiento. Las rutas dinámicas pueden tener caché lazy en RAM; el contenido estático no.
- Antes de añadir cada caché se comprueba si el uso de IDs ha eliminado el cálculo. El tamaño y el límite de la caché se decidirán con medidas, no con una reserva por nodo.

## Implementación incremental dentro del milestone

1. **Arena estructural.** Retirar el prototipo de IDs incrustados en `Object`. Definir IDs, registros de nodos, rangos de hijos y enlaces de nombre/padre. Construir y comprobar este arena desde el parser existente sin retener `Rc` en los registros. Mientras se migra el intérprete, el camino JSON antiguo puede seguir activo, pero el arena de transición no se conserva dentro de `Story` para no duplicar RAM.
2. **Contenido propio.** Trasladar todos los tipos de nodo y sus operandos al arena. Adaptar ambos parsers JSON para construirlo, sin árbol intermedio persistente. Finalizar referencias estáticas y validar IDs, rangos, nombres y destinos. Mantener el orden de hijos y los contenedores solo nombrados.
3. **Intérprete por IDs.** Sustituir `Pointer`, navegación, desvíos, elecciones y operaciones por accesos al arena. Separar valores dinámicos del contenido. Eliminar `Rc<dyn RTObject>` de la ruta de ejecución estática; permitir ownership compartido solo donde el estado o la API pública lo requieran.
4. **Estado y límites.** Pasar contadores y parches a IDs; conservar JSON de guardado/carga mediante conversión en los bordes. Añadir solo las cachés lazy de estado que todavía resulten necesarias. Eliminar el árbol y sus cachés de los constructores de `Story`.
5. **Equivalencia y coste.** Comparar historias, elecciones, listas, funciones, variables, flujos y guardado/carga con el comportamiento anterior. Medir tamaño del arena y asignaciones al crear/ejecutar una historia; comprobar `no_std + alloc` y los dos parsers. Documentar que el backend JSON aún ocupa RAM, mientras que la ausencia de reconstrucción desde flash corresponde al milestone 3.

## Criterio de cierre

Los constructores JSON y las API públicas conservan su comportamiento. El intérprete usa IDs para el contenido estático; `Story` no retiene un árbol de `Rc<dyn RTObject>` ni cachés mutables dentro de nodos estáticos. Las rutas y contadores no se recalculan en el camino habitual de ejecución. El formato de estados guardados sigue siendo compatible. `cargo fmt --all` y Clippy estricto pasan; la validación funcional y de `no_std` se realiza antes de declarar cerrado el milestone.

## Estado

Milestone completado. El arena posee la estructura, los operandos de los tipos de nodo, las listas y sus definiciones. Resuelve rutas y enlaza destinos estáticos por ID. Ambos lectores JSON construyen el arena directamente: Serde a partir de sus tokens JSON y el lector por streaming a medida que consume la entrada. La comparación con el parser anterior sobre *The Intercept* cubre tipos y operandos de nodo, jerarquía, nombres, flags y destinos. La lectura del arena se concentra en sus accesores por ID; la vista de bytes de la imagen se añadirá en el milestone 3.

`Story` usa ahora el intérprete plano por defecto; `LegacyStory` conserva temporalmente la implementación anterior para comparaciones. El intérprete ejecuta texto, glue, expresiones, variables, elecciones, hilos, túneles, funciones Ink, listas, etiquetas, contadores, secuencias aleatorias y llamadas externas. Inicializa las variables globales sin el árbol, mantiene flujos separados, difiere las llamadas externas no seguras durante el lookahead y permite pausar y reanudar `continue_async`. Las elecciones convertidas para la API pública se cachean de forma lazy en la instancia, sin cachés en nodos estáticos. Los códecs Serde y streaming leen y escriben el formato de estado Ink mediante IDs y rutas en el límite; se han probado intercambios de estados con elecciones entre el intérprete anterior y el plano, además de varios flujos e hilos. Las pruebas diferenciales comparan más de cien historias compiladas y una partida completa de *The Intercept* con el intérprete anterior.

La suite completa del workspace, la suite del runtime con `stream-json-parser`, la compilación `no_std + alloc` para `thumbv7em-none-eabihf`, `cargo fmt --all` y Clippy estricto pasan. `Story::new*` no retiene el árbol de `Rc`; `LegacyStory` sigue compilando como referencia durante la transición. El JSON todavía se parsea al crear la historia y el arena sigue en RAM. La imagen en flash y la eliminación de ese parseo corresponden a los milestones 2 y 3.

## Medición provisional (host x86_64, release)

El ejemplo `runtime/examples/flat_story_cost.rs` mide bytes de heap solicitados al asignador, pico durante construcción y primera ruta de *The Intercept*. El JSON se incluye en el binario y no se cuenta como heap. Se ejecutaron cinco pasadas; los tiempos siguientes son los valores centrales de las cuatro pasadas tras la primera. Son medidas del host, no un perfil real del ESP32 ni una estimación de fragmentación de PSRAM.

| Parser | Intérprete | Heap retenido | Pico de heap | Construcción | Primera ruta |
| --- | --- | ---: | ---: | ---: | ---: |
| Serde | árbol | 1 830 419 B | 3 819 439 B | ~3,7 ms | ~1,6 ms |
| Serde | plano | 986 175 B | 3 114 107 B | ~4,1 ms | ~0,51 ms |
| Streaming | árbol | 1 868 607 B | 1 875 343 B | ~3,2 ms | ~1,5 ms |
| Streaming | plano | 971 839 B | 1 115 431 B | ~4,2 ms | ~0,51 ms |

El arena plano reduce el heap retenido cerca de un 48 % con el lector por streaming y mantiene el pico de construcción alrededor de 1,12 MB en este ejemplo. El lector Serde sigue creando un pico transitorio de ~3,11 MB. La construcción del arena con streaming tarda algo más en esta máquina; la ejecución de la primera ruta es aproximadamente tres veces más rápida. Hay que repetir la medida en el ESP32-S3 con el asignador, la PSRAM, el firmware y el asset finales antes de afirmar que cabe en 2 MB.
