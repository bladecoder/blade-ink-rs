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
- [ ] Game Queries and Functions
- [x] Nested flows
- [ ] Variables and Logic
- [x] Conditional blocks (if/else)
- [x] Temporary Variables
- [x] Functions
- [x] Tunnels
- [x] Threads
- [x] Tags
- [x] Lists
- [ ] Load/Save state

## TODO

- [ ] Test for flow
- [ ] Use OnceCell to lazy init the cache fields of RTObjects
- [ ] Error handling
- [ ] Split large files. ex. Get the error handling out of the Story class. The performLogic 



