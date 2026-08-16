# 0082 — docs.rs documents skylode-core's private items, deliberately

**Status:** accepted
**Tags:** project
**Supersedes:** —
**Superseded by:** —

## Decision

docs.rs documents `skylode-core`'s private items, deliberately

## Why

The rustdoc here argues *why* an item's visibility is what it is, so it has to name
`pub(crate)` items — `set_richness_setting` is `pub` precisely *unlike* `take`.
`--document-private-items` is therefore set both on docs.rs and locally (`cargo
doc-all`), and `rustdoc::private_intra_doc_links` is allowed because its premise, a
reader without the flag, no longer exists. The cost is accepted knowingly: a consumer
browsing docs.rs sees internals beside the API and must read the signature to tell them
apart.
