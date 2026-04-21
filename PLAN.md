# Plan Para Terminar El Port Del Compilador Ink

## Estado Actual
- `Wave 0` a `Wave 5`: completadas como infraestructura incremental.
- `Wave 6`: completada. Ya existe base de referencias parent/content, índice de objetos, búsqueda profunda, ancestry/story/flow resolution, errores/warnings deduplicados, estado de cache runtime y guard `BLADEINK_DISABLE_BOOTSTRAP=1`.
- `Wave 7`: completada. `Compiler::parse()` devuelve una jerarquía poblada con globals, listas, externos, root nodes, flows, includes resueltos y refs indexables.
- `Wave 8`: completada y realineada. El compilador nuevo ya construye objetos del runtime para el subconjunto básico y serializa con `RuntimeStory::to_compiled_json()`, sin emitir JSON manual.
- Todos los conformance tests pasan hoy en modo híbrido.
- El compilador nuevo directo solo cubre el subconjunto de `wave1`: texto, newlines, glue, knots/stitches y diverts simples.
- El resto sigue pasando porque `Compiler` cae al compilador legacy en `compiler/src/bootstrap`.
- Definición de terminado: todos los tests deben pasar sin usar `bootstrap`.

## Objetivo
- Sustituir la ruta híbrida por el pipeline del compilador C#:
  `pre_parse -> parse -> post_parse -> resolve_references -> export_runtime -> post_export`.
- Mantener `bootstrap` temporalmente solo como oracle de comparación y fallback medible.
- Borrar `bootstrap` y `wave1` al final, cuando la ruta nueva esté verde.

## Waves Pendientes

### Wave 6: Base Real De `ParsedObject`
- Estado: completada.
- Implementado:
  - `ParsedObjectRef` para referencias de árbol.
  - `ParsedObjectIndex` para resolver ancestry, story root, flow más cercano y búsquedas profundas.
  - Parent refs, content refs, búsqueda directa por `ObjectKind`, ancestry resoluble por callback, `story_ref` y `closest_flow_base`.
  - Errores/warnings deduplicados por objeto.
  - Marcado básico de cache runtime y path runtime opcional.
  - Guard `BLADEINK_DISABLE_BOOTSTRAP=1` para impedir fallback legacy.
  - Refs parent/content integradas en los nodos base ya existentes.
- Portar la semántica base de `ParsedHierarchy/Object.cs`:
  `content`, parent, ancestry, `story()`, `closest_flow_base`, `find/find_all`, errores/warnings, runtime object cacheado y runtime path.
- Unificar los nodos actuales para que participen en un árbol común y puedan exponer sus hijos de forma consistente.
- Añadir un modo test `BLADEINK_DISABLE_BOOTSTRAP=1` que haga fallar cualquier fallback a `bootstrap`.
- Gate:
  - `cargo test -p bladeink-compiler`
  - Unit tests nuevos para parent/ancestry/find/runtime-cache/error propagation.
  - `cargo test` completo en modo híbrido.

### Wave 7: Parser Completo A `ParsedHierarchy`
- Estado: completada.
- Implementado:
  - `ParsedNode`, `ParsedNodeKind`, `ParsedExpression` y `ParsedFlow` como árbol parseado completo e indexable.
  - Conversión completa desde el parser interno existente a `ParsedHierarchy`.
  - `Compiler::parse()` usa `InkParser::parse_story()` y ya no devuelve un `Story` vacío.
  - `Compiler::parse()` respeta `CompilerOptions.file_handler` para resolver `INCLUDE`.
  - Tests de inspección del árbol parseado para globals, listas, externos, choices, gathers, flows e includes.
- Hacer que `Compiler::parse()` use `InkParser::parse()` real y devuelva una `ParsedStory` poblada.
- Portar ensamblaje de statements, flows, knots/stitches, includes, choices/gathers/weave, logic lines, inline logic, tags, sequences y listas.
- Mantener fallback solo en `compile`, no en `parse`.
- Gate:
  - Tests nuevos que inspeccionen el árbol parseado.
  - Includes, errores y stats en verde con fallback.

### Wave 8: Expresiones Y Runtime Básico
- Estado: completada.
- Implementado:
  - Export runtime directo desde `ParsedHierarchy::Story` en `compiler/src/runtime_export.rs`.
  - Construcción real de `Runtime.Container` y `Runtime.Object` (`Value`, `Glue`, `Divert`, `ControlCommand`, `VariableAssignment`, `VariableReference`, `NativeFunctionCall`) siguiendo la arquitectura C#.
  - Serialización exclusivamente vía `RuntimeStory::to_compiled_json()`; se eliminó el exportador JSON manual `basic_export.rs`.
  - Root container, `global decl`, named content de knots/stitches simples y `listDefs` creados como objetos runtime.
  - `Text`, newlines, glue, output expressions, diverts simples, `END`, `DONE` y variable-diverts globales.
  - Inicialización de globals, constants usados en expresiones, list definitions e initial list values.
  - `VariableAssignment` runtime para `Set`, `TempSet`, `AddAssign` y `SubtractAssign`.
  - Expresiones escalares, strings, variables, divert targets, listas, lista vacía, unary `!`/`-`, binarios aritméticos/comparación/bool/lista y builtins básicos de listas.
  - Llamadas a listas declaradas (`List()` y `List(n)`) y builtins `LIST_VALUE`, `LIST_COUNT`, `LIST_MIN`, `LIST_MAX`, `LIST_ALL`, `LIST_INVERT`, `LIST_RANGE`, `LIST_RANDOM`.
  - Runtime reexporta los tipos necesarios para que el compiler no duplique la serialización ni los tokens JSON.
  - Test oracle con `inklecate` en PATH compara JSON compilado por C# contra la ruta nueva Rust para texto, divert simple y listas básicas.
- Activado en modo `BLADEINK_DISABLE_BOOTSTRAP=1` para validar el subconjunto portado sin fallback; en modo normal el pipeline conserva `bootstrap` como ruta híbrida segura hasta que Waves 9-12 eliminen las features avanzadas pendientes.
- Gates sin fallback verdes:
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test basic_text_test`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test glue_test simple_glue_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test glue_test glue_with_divert_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test divert_test simple_divert_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test divert_test invisible_divert_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test divert_test same_line_divert_is_inline_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test variable_test variable_declaration_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test variable_test arithmetic_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test variable_test bools_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test variable_test const_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test variable_test string_constants_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test variable_test string_contains_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test variable_test increment_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test variable_test var_calc_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test variable_test temporaries_at_global_scope_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test variable_test variable_divert_target_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test list_test empty_list_origin_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test list_test empty_list_origin_after_assinment_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test list_test list_basic_operations_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test list_test list_mixed_items_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test list_test more_list_operations_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test list_test list_save_load_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test list_test list_range_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test list_test list_all_bug_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test list_test contains_empty_list_always_false_test -- --exact`
  - `BLADEINK_DISABLE_BOOTSTRAP=1 cargo test -p conformance-tests --test list_test list_random_test -- --exact`
- Tests aún fuera de Wave 8:
  - `glue_test` avanzado: condicionales inline y funciones.
  - `divert_test` avanzado: choices, gathers, weave points y labels.
  - `variable_test` avanzado: conditionals, functions, tunnels, parameterised diverts y weave labels.
  - `list_test` restante: choices/weave y parser legacy interno para listas con valores explícitos o múltiples listas relacionadas.

### Wave 9: Flow, Weave, Choices Y Gathers
- Portar `FlowBase`, `Story`, `Weave`, `Choice`, `Gather`, `Wrap`, loose ends, default choices, flags de visit/read counts y naming de weave points.
- Implementar `ContentWithNameAtLevel`, paths relativos, containers para counting y resolución de labels internos.
- Activar compilación nueva sin fallback para choices, gathers, knots, stitches y weave.
- Gate sin fallback:
  - `choice_test`
  - `gather_test`
  - `knot_test`
  - `stitch_test`
  - casos de `misc_test` ligados a weave/naming/visibility.

### Wave 10: Condicionales, Secuencias, Túneles Y Threads
- Portar generación runtime y resolución de:
  `Conditional`, `ConditionalSingleBranch`, `Sequence`, `Divert`, `Return`, `TunnelOnwards`,
  threads, tunnels y functions con args/ref/divert targets.
- Implementar validaciones equivalentes al C# para flow calls, tunnel returns, variable divert targets y call stack.
- Activar compilación nueva sin fallback para funciones, tunnels, threads y multiflow.
- Gate sin fallback:
  - `conditional_test`
  - `variable_text_test`
  - `function_test`
  - `tunnel_test`
  - `thread_test`
  - `multi_flow_test`
  - `runtime_test`

### Wave 11: Resolución Completa, Plugins, Stats Y CLI
- Completar `ResolveReferences` siguiendo el orden C#:
  constants/list defs, variable declarations, naming collisions, list item resolution,
  external declarations, includes y debug metadata.
- Integrar plugins en el pipeline real.
- Hacer `Stats::generate_from_parsed` equivalente al C#.
- Adaptar `rinklecate` a la ruta directa `Compiler::compile_story()` y usar JSON solo vía `Story::to_compiled_json()`.
- Gate sin fallback:
  - `list_test`
  - `misc_test` completo
  - `cargo test -p rinklecate`
  - compilación real de `TheIntercept.ink`

### Wave 12: Retirar Bootstrap Y Cerrar Paridad
- Añadir test guard que asegure que `Compiler` no llama a `bootstrap`.
- Comparar salida compilada contra el oracle legacy antes de borrarlo para una muestra fija:
  listas, choices, tunnels, functions, includes y `TheIntercept.ink`.
- Borrar `compiler/src/bootstrap`, `compiler/src/wave1.rs` y cualquier conversión JSON intermedia del compilador.
- Gate final:
  - `cargo test`
  - `cargo test -p conformance-tests`
  - `cargo test -p rinklecate`
  - compilación real de `TheIntercept.ink`
  - todo sin fallback.

## Reglas De Ejecución
- Cada wave debe cerrar con tests en verde.
- Mientras exista `bootstrap`, cada wave debe ejecutar tests en modo híbrido y el subconjunto ya portado en modo `BLADEINK_DISABLE_BOOTSTRAP=1`.
- Cada feature portada debe añadir al menos una comparación JSON estructural contra `inklecate` cuando la salida compilada sea estable.
- No se considera completada una feature si solo funciona por fallback.
- La fidelidad al compilador C# prima sobre rediseñar la arquitectura.
