# Packages — Spec

## Status

Active.

This spec records the runtime-owned package boundary for the foundation and standard event vocabulary. It does not authorize package webviews, loaders, or agent execution.

## 1. Runtime event vocabulary

The runtime owns standard event names. Packages may consume declared standard events but cannot receive installation private keys, workspace tokens, raw Iroh handles, filesystem paths, or unvalidated membership data.

The peer lifecycle vocabulary is `peer:joined`, `peer:left`, and `peer:connection`. `peer:connection` carries only a public member identifier and a secret-free connection/presence state. The runtime derives every event from the active workspace session; packages do not infer membership from gossip traffic.
