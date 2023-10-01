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

- [ ] Variable observers.
- [ ] Optimize control command getname. Use static string array and address it by order.
- [ ] Error handling
- [ ] Cache components string in Path
- [ ] Use OnceCell to lazy init the cache fields of RTObjects
- [ ] Split large files. ex. Get the error handling out of the Story class. The performLogic 
- [ ] Story.state y VariablesState.default_global_variables shouldn't be optionals.
- [ ] Review all the .unwrap() and change it by .ok_or("xxx"). We need to avoid panics!
- [ ] Multi-flow methods.


