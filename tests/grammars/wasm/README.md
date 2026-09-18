# Offline WASM grammar fixtures

These unmodified upstream release assets are used by tests, never embedded in
release binaries or included in the published treetags crate. See
[provenance.json](provenance.json) for pinned versions, URLs, ABIs, and SHA-256
checksums, and the adjacent license files for their MIT notices.

Tests copy these files into temporary `XDG_CONFIG_HOME` directories. No network
access or grammar build toolchain is needed to run the grammar tests.

OCaml's matching query is vendored in `queries/ocaml.scm`, with its license in
`queries/LICENSE-ocaml`. All grammars use ABI 14 even though the runtime also
supports newer ABIs. Updating a fixture requires updating its descriptor, query
if applicable, checksum, and baseline-output compatibility tests together.
