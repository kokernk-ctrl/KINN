# KINN module & package system — specification and foundation (0.3)

The ecosystem layering, what exists in 0.3.0, and what is deliberately
not built yet. Rule: nothing here is claimed to work without
implementation + tests; third-party interoperability stays **outside**
the Kin syntax (ADR-0022).

---

## 1. Layering

```
 ┌─────────────────────────────────────────────────────┐
 │ applications (.kin, website / future ui modules)    │
 ├─────────────────────────────────────────────────────┤
 │ standard library (natives today; kinn::std planned) │
 ├─────────────────────────────────────────────────────┤
 │ module system        import "path.kin" as alias;    │
 ├─────────────────────────────────────────────────────┤
 │ package manager      kinn.toml manifests, (planned) │
 │                      dependency resolution          │
 ├─────────────────────────────────────────────────────┤
 │ registry             (planned, after the package    │
 │                      manager is real)               │
 ├─────────────────────────────────────────────────────┤
 │ target backends      web (0.3), android (0.3), …    │
 ├─────────────────────────────────────────────────────┤
 │ network / graphics / native FFI APIs                │
 │ (planned: 0.5 graphics per ROADMAP; FFI exists for  │
 │ hosts, not yet as a user-facing Kin API)            │
 └─────────────────────────────────────────────────────┘
```

## 2. Modules (implemented, 0.2+)

- `import "file.kin" as alias;` — path-based, resolved relative to the
  importing file, confined inside the project (E3017: no `..`, no
  absolute paths, no backslashes).
- Transitive loading with diamond sharing; cycles are E3018 with the
  full chain. Duplicate alias E3019, unknown module/function
  E3020/E3021.
- Modules export **functions only** (ADR-0015); imported `main` is
  inert. Qualified calls `m.f(...)`.
- Loader-callback based (`kinc::project`), so CLI (filesystem),
  Studio web (virtual FS) and Android (app storage) share identical
  semantics — verified by tests on all three paths.

## 3. Package manifest (implemented in 0.3)

`kinn.toml` in a project directory:

```toml
# KINN project manifest
name = "my-site"
version = "0.1.0"
entry = "site.kin"
target = "web"        # default build target for `kinn build`
```

- Strict minimal subset: top-level `key = "value"` string pairs, `#`
  comments; the first `[section]` ends top-level parsing (forward
  compatibility). Unquoted values and broken lines are rejected with a
  line number (unit-tested).
- `kinn build` with no file argument reads `./kinn.toml` and builds
  `entry` with `target` as the default.
- Unknown keys are ignored so older CLIs keep reading newer manifests.

## 4. Package manager — planned (not in 0.3)

Next steps, in order, each gated on tests before the next:

1. dependency table in the manifest (`[dependencies]`),
2. a resolution algorithm over local paths (`path = "../lib"`),
3. a lockfile,
4. registry fetch last — only once resolution is proven offline-first.

No dependency download, no lockfile, no registry exist in 0.3 and
nothing pretends to.

## 5. Registry — planned

A registry is an append-only index mapping `name + version` to
content hashes. Design constraints (from the product rules):
offline-first caching, content-addressed packages, no account
required for publishing to a self-hosted mirror. Not started.

## 6. Standard library — current state

`len`, `abs`, `min`, `max`, `print` are compiler natives (type-checked
calls, `NativeId` in KIR). A growing `kinn::std` with string/arrays
helpers is planned; string interpolation and string→number casts are
decided deferrals (ADR-0013, P-013/P-015 in the registry).

## 7. Network / Graphics / Native interop — planned

- **Graphics**: OpenGL ES 3.0 first (ADR-0010), milestone 0.5.
- **Network APIs**: nothing exists; any future API must work through
  explicit capability declarations and keep the offline-first rule
  (apps run without network; network features degrade honestly).
- **Native interop**: the FFI (`kinn-ffi`, C ABI) is currently a
  *host* bridge (wasm/JNI/native tests). A user-facing
  `external`-function story is future work and will be specified
  separately from language syntax (no inline C/Kotlin/JS in .kin).

## 8. SDK

The SDK is the distribution shape: compiler + CLI + stdlib + Studio +
backends, versioned together (0.3.0 artifacts from CI). A separate
`kinn-sdk` packaging with headers for host embedding is planned after
the package manager.
