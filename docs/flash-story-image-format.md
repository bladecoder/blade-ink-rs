# Formato binario de la historia (versión 1)

El generador `bladeink::image::compile_json_to_image` recibe un `.ink.json` compilado y escribe una imagen determinista. `rinklecate --image` permite partir de `.ink` o `.ink.json`. Esta versión define **cómo se escribe** el archivo; la lectura directa desde flash se implementará en el Milestone 3.

Todos los enteros ocupan 32 bits little endian. Los offsets son absolutos desde el principio de la imagen; dentro de un registro, los offsets de cadenas y payloads son relativos a la sección correspondiente. El valor `0xffffffff` indica un campo opcional ausente. No se serializan punteros ni representaciones de estructuras Rust.

## Cabecera (88 bytes)

| Offset | Contenido |
| ---: | --- |
| 0 | Magic ASCII de 8 bytes: `BLINKIMG` |
| 8 | Versión de formato: `1` |
| 12 | Versión Ink (`i32`) |
| 16 | Longitud total en bytes |
| 20 | Offset y número de nodos |
| 28 | Offset y número de IDs de hijos ordenados |
| 36 | Offset y número de hijos con nombre |
| 44 | Offset y número de definiciones de listas |
| 52 | Offset y número de elementos de definiciones |
| 60 | Offset y número de rutas de contenedores |
| 68 | Offset y longitud de payloads |
| 76 | Offset y longitud de cadenas |
| 84 | Reservado, cero |

Las secciones se concatenan en el orden de la cabecera. Los nodos, hijos, hijos con nombre, definiciones, elementos y rutas ocupan 40, 4, 12, 16, 12 y 12 bytes por entrada, respectivamente. El ID de un nodo es su posición en la sección de nodos; el ID `0` es la raíz.

## Nodos

Cada nodo contiene diez palabras: `parent`, `child_index`, `tag` y siete campos `f0` a `f6`. Los dos primeros pueden ser `0xffffffff`. Un par `str` representa `(offset, longitud)` dentro de la sección de cadenas UTF-8. Una ruta se guarda como texto Ink, incluido el `.` inicial si es relativa. Los destinos estáticos resueltos se guardan como IDs.

| Tag | Tipo | Campos usados |
| ---: | --- | --- |
| 1 | Contenedor | `f0:f1` nombre opcional; `f2` flags; `f3:f4` inicio y número de hijos; `f5:f6` inicio y número de hijos con nombre |
| 2 | Elección | `f0:f1` ruta opcional; `f2` destino; `f3` flags |
| 3 | Comando Ink | `f0:f1` nombre del comando |
| 4 | Desvío | `f0:f1` ruta opcional; `f2` destino; `f3:f4` nombre de variable opcional; `f5` argumentos externos; `f6` flags |
| 5 | Glue | Sin campos |
| 6 | Operación nativa | `f0:f1` nombre de la operación |
| 7 | Etiqueta | `f0:f1` texto |
| 8 | Booleano | `f0` = 0 o 1 |
| 9 | Entero | `f0` como `i32` |
| 10 | Float | `f0` bits IEEE-754 de `f32` |
| 11 | Texto | `f0:f1` texto |
| 12 | Lista | `f0:f1` offset y longitud del payload |
| 13 | Destino como valor | `f0:f1` ruta; `f2` ID resuelto |
| 14 | Puntero a variable | `f0:f1` nombre; `f2` índice de contexto |
| 16 | Asignación | `f0:f1` nombre; `f2` flags |
| 17 | Referencia de variable | `f0:f1` nombre; `f2:f3` ruta de recuento opcional; `f4` destino de recuento |
| 18 | Void | Sin campos |

En desvíos, los bits 0, 1 y 2 de `f6` indican condición, llamada externa y apilado; los bits desde 3 codifican `PushPopType` (túnel = 0, función = 1, evaluación desde juego = 2). En asignaciones, los bits 0 y 1 indican variable global y declaración nueva. Los campos sin usar contienen `0xffffffff`.

## Índices y datos variables

- **Hijos ordenados:** un `u32` con el ID de cada hijo. El contenedor apunta a un rango de esta sección.
- **Hijos con nombre:** `(offset de nombre, longitud, ID)`; el rango de cada contenedor está ordenado por nombre para búsqueda binaria.
- **Definiciones de listas:** `(offset de nombre, longitud, inicio de elementos, número de elementos)`, ordenadas por nombre. Cada elemento es `(offset de nombre, longitud, valor i32)` y los elementos de una definición están ordenados por nombre.
- **Rutas:** `(offset de ruta, longitud, ID de contenedor)`, ordenadas por el texto de la ruta. Incluyen la raíz (`""`) y permiten buscar las rutas usadas en el estado guardado. Los punteros a hijos añaden su componente numérico final.
- **Payload de lista:** `n_items`, `n_origins`, seguido de `n_items` entradas `(offset de origen, longitud, offset de nombre, longitud, valor i32)` y `n_origins` pares `(offset, longitud)`. Los offsets y longitudes se expresan en bytes.
- **Cadenas:** bytes UTF-8 concatenados, sin terminador. Se reutiliza el primer offset de cada cadena repetida.

El generador rechaza el JSON inválido, las referencias estáticas irresolubles y los tamaños que no caben en campos de 32 bits. La futura vista binaria validará todos los límites y referencias antes de ejecutar la historia.
