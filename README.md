# blade-ink-rs
Inkle Ink runtime implementation in Rust

Currently under development. This is the implementation status:

- [x] Loading .json file
- [x] Show plain lines (no logic nor choices)
- [x] Choices
- [x] Knots and Stitches
- [x] Diverts
- [x] Variable Text
- [x] Conditional Text
- [x] Game Queries and Functions
- [x] Nested flows
- [x] Variables and Logic
- [x] Conditional blocks (if/else)
- [x] Temporary Variables
- [x] Functions
- [x] Tunnels
- [x] Threads
- [x] Tags
- [x] Lists
- [x] Load/Save state

## TODO

- [ ] Cache components string in Path
- [ ] Variable observers.
- [ ] External functions.
- [ ] Doc

- [ ] story.state -> quitar el pub de get_state()/mut y que guardar/salvar sea pub(crate). Crear fichero con pub methods??
- [ ] Use OnceCell to lazy init the cache fields of RTObjects
- [ ] Split large files. ex. Get the error handling out of the Story class. The performLogic 
- [ ] Review all the .unwrap()s and change them by .ok_or("xxx"). We need to avoid panics!

