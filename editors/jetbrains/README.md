# rqx for JetBrains IDEs

A thin client: it starts `rqx lsp` and lets [LSP4IJ][lsp4ij] speak the protocol.
Diagnostics, hovers, quick fixes, the code lens and the command set all come from the
server, so a capability added there appears here without a release of this.

LSP4IJ rather than the platform's own LSP API, because that one is Ultimate-only —
which would leave out PyCharm CE and IDEA CE, most of the people who would install
this.

## Status

Written but not yet built: the CI job in `.github/workflows/editors.yml` is the first
thing that compiles it, because doing so needs Gradle and the IntelliJ Platform SDK.
Treat the first green run of that job as the point this becomes real.

## Building it

```bash
cd editors/jetbrains
./gradlew buildPlugin      # build/distributions/rqx-<version>.zip
./gradlew runIde           # a sandbox IDE with the plugin loaded
./gradlew verifyPlugin     # what the Marketplace checks before it accepts one
```

## The binary

The plugin ships none. One zip carrying five platforms would be fifty megabytes for
the one you are on, and the release assets are already named by target for the other
editors. It looks in `rqx.path`, then `PATH`, then `target/release/rqx` for anyone
working on rqx itself.

[lsp4ij]: https://github.com/redhat-developer/lsp4ij
