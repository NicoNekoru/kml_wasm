# Kernel ML (KML) parser

Canonical Rust implementation of **Kernel ML** (KML). The package exposes a native compiler API and a WASM build for browser clients. It emits body-only HTML snippets, leaving full-page rendering, styling, sanitisation policy, and MathJax loading to the host application.

The KML language spec, implementation notes, tests, and showcase fixture are vendored in [`spec/`](spec/), so the crate can be cloned, tested, and built without relying on sibling directories.

## Prerequisites

- [Rust](https://www.rust-lang.org/) and `cargo`
- [`wasm-pack`](https://rustwasm.github.io/wasm-pack/) for WASM builds

## Native compiler

```bash
cargo run --bin compile_kml -- path/to/file.kml
# or:
cat path/to/file.kml | cargo run --bin compile_kml
```

The command prints the compiled body HTML snippet to stdout and reports compile errors with line, column, and byte offset diagnostics.

## Rust API

```rust
let html = kml_wasm::compile_inner("# Title")?;
```

Use `compile_inner(source)` from native Rust code when you want a `Result<String, CompileError>`.

## WASM build

```bash
rustup target add wasm32-unknown-unknown
wasm-pack build --target bundler
```

Output is written to `pkg/` (`kml_wasm.js`, `kml_wasm_bg.wasm`, and package metadata). `pkg/` is generated and intentionally ignored by git.

Browser/bundler usage:

```ts
import init, { compile } from 'kml_wasm'

await init()
const html = compile('# Title')
```

For interactive editors, use the stateful `LiveCompiler` export. It keeps parsed
AST blocks for unchanged top-level chunks and re-emits the full document so
global output such as footnote numbering remains correct.

```ts
import init, { LiveCompiler } from 'kml_wasm'

await init()
const compiler = new LiveCompiler()
const html = compiler.render(source)
const stats = JSON.parse(compiler.stats_json())
```

Host applications can consume the generated package through a file dependency, a workspace package, a published package, or by serving the generated files from a public assets directory.

## KML spec

- [`spec/language.md`](spec/language.md): user-facing syntax and parsing model.
- [`spec/implementation.md`](spec/implementation.md): compiler pipeline notes.
- [`spec/tests.md`](spec/tests.md): language behavior test notes.
- [`spec/showcase.kml`](spec/showcase.kml): canonical showcase fixture used by tests.

## Tests

```bash
cargo test
```

## Troubleshooting

- **`linker wasm-ld not found` / missing WASM target:** Run `rustup target add wasm32-unknown-unknown`. If you use Homebrew Rust instead of Rustup, see the wasm-pack documentation for non-rustup setups.
- **Host app cannot import `kml_wasm`:** Build the WASM package first with `wasm-pack build --target bundler`, then point the host dependency or alias at the generated `pkg/` directory.
