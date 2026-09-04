# Third-party code

Everything rqx ships is its own except the files listed here.

## `crates/rqx-cli/src/serve/app/vendor/preact.js`

Preact 10.24.3, Preact Hooks and htm 3.1.1, concatenated and unmodified.

- Preact — MIT — <https://github.com/preactjs/preact>
- htm — Apache-2.0 — <https://github.com/developit/htm>

Vendored rather than fetched from a CDN: the dashboard is served by a binary that
promises no runtime dependencies, and a page that reached the network to render would
not keep that promise on a machine without one.

Rust dependencies are declared in `Cargo.toml` and their licences are in `Cargo.lock`;
`cargo deny` or `cargo license` will list them.
