# Languages and repository surfaces

The native registry recognizes 24 named source surfaces across 92 registered
extensions. Support is evidence-specific: lossless source recovery does not
pretend to provide the same semantic depth as a typed adapter.

## Code and schema adapters

| Surface | Evidence extracted |
| --- | --- |
| Rust | modules, `use`/re-exports, items, impl ownership, traits, scope-bound calls (bare names, `self`/`super`/`crate` paths, `self.method()`) including inside standard formatting macros, tests, Axum/Actix/Rocket-style routes |
| JavaScript / JSX | ESM/CommonJS imports and exports, declarations, members, calls, routes, event clients, and package frameworks (Next.js, Nuxt, React Native, Expo, Electron, Nest, Express, Fastify, and the other package.json frameworks) |
| TypeScript / TSX | JavaScript evidence plus interfaces, type-only coupling, aliases, and typed member ownership |
| Python | imports, declarations, classes, calls, decorators, Django/Flask/FastAPI routes, PyTorch and other ML framework imports, `nn.Module` models, and messaging |
| Go | packages, imports, declarations, receivers, calls, HTTP, and gRPC evidence |
| Java | packages, classes, methods, inheritance, Spring endpoints, JMS, and event clients |
| C# | namespaces, types, members, calls, attributes, and routes |
| C | includes, declarations, functions, calls, and bindings into Java (JNI), Go (cgo), C# (P/Invoke), Python (pybind11, nanobind, cffi), Node (node-addon), and Rust (cxx) |
| C++ | C evidence plus namespace/class structure, native frameworks (Qt, Boost, OpenCV, Unreal, Godot, SDL, GTK, gRPC, CUDA, JUCE, and the other include markers), and cpp-httplib, Crow, Drogon, and oatpp routes |
| SQL | schemas, tables, fields, statements, and host references |
| Bash / Zsh | commands, functions, variable references, and script structure |
| Swift | same-module files, imports, types, heritage, calls, URLSession/WebSocket client routes, and an App Store `@main` entry |
| Solidity | contracts, functions, events, and calls |

## Contracts and configuration

| Surface | Evidence extracted |
| --- | --- |
| GraphQL | schema types, fields, root operations, executable calls, fragments, and referenced return types |
| Protobuf / gRPC | packages, messages, enums, services, RPC input/output types, and unary/client/server/bidirectional streaming modes |
| JSON / JSONC | lossless syntax plus recognized package, compiler, architecture, and application configuration; after parse, n8n workflow JSON is a domain, not a second `.json` adapter |
| YAML / Kubernetes | documents, deployment resources, services, workloads, images, ports, and configuration links; after parse, Dify DSL is a domain, not a second `.yml` adapter |
| Terraform / HCL | blocks, resources, providers, variables, outputs, references, and deployment evidence |
| XML | elements, attributes, configuration structure, Info.plist and ExportOptions application identity, and AndroidManifest.xml |

## Documents and UI sources

| Surface | Evidence extracted |
| --- | --- |
| HTML / Vue / Svelte | lossless markup structure, elements, attributes, embedded links, and component-facing context |
| CSS / SCSS / Sass / Less | selectors, declarations, imports, variables, and lossless style syntax |
| Markdown / MDX | headings, links, code fences, embedded JSX boundaries, document structure, and recognized Mermaid flowchart regions after parse |
| Mermaid | standalone `.mmd`/`.mermaid` flowcharts: native IDs, labels, subgraphs, and `declared_architecture` arrows |
| reStructuredText | sections, directives, links, and source-preserving document structure |
| AsciiDoc | sections, attributes, includes, links, and source-preserving document structure |

## Cross-surface domain evidence

Adapters and operation passes connect source facts to repository domains:

- Mermaid flowchart regions as `declared_architecture`, never as production
  calls;
- HTTP route handlers, mount chains, client calls, and cross-repository
  contracts; a client and a server of the same `METHOD /path` share one endpoint;
- App Store targets, StoreKit products, entitlements, privacy manifests, Android
  applications, and framework/application nodes (`FRAMEWORK`, `APP`, `MODEL`);
- C and C++ frameworks and the same framework node from the other language that
  binds them. Studio specs share those nodes: Visual Studio (`.sln`, `.vcxproj`),
  Qt (`.pro`, `.pri`), Unreal (`.uproject`, `.uplugin`), JUCE (`.jucer`), Godot
  (`.gdextension`), Unity (`.asmdef`), CMake (`.cmake`), Code::Blocks (`.cbp`),
  and Xcode (`.xcconfig`). `CMakeLists.txt`, `Makefile`, `meson.build`, and
  `conanfile.txt` are read by file name. `vcpkg.json` names the vcpkg app;
- GraphQL operations and referenced schema types;
- Protobuf messages, gRPC services, methods, and streaming modes;
- Kafka, RabbitMQ/AMQP, JMS, NATS, SQS, and SNS producers, consumers, topics,
  queues, exchanges, and bindings;
- MongoDB usage, package manifests, lockfiles, architecture contracts, and
  measured coverage artifacts.

## Precision boundary

Parsed syntax, manifests, and exact spans are deterministic. Cross-file
resolution uses language scope, imports, aliases, ownership, re-export chains,
module paths, and callable kinds.

Dynamic dispatch that cannot be proven stays unresolved. An ambiguous name is
not connected to an arbitrary same-named symbol. Static reachability is not
reported as measured coverage, and optional evidence that is absent remains
explicitly absent.

A Rust call binds only to a name the referencing file can name: a bare
identifier, a `self`, `super` or `crate` path, or `self.method()`, whose
receiver type is proven by the enclosing impl block. `values.push(item)` is
`Vec::push` and `Repository::open(root)` ends in another type's namespace, so
neither binds to a same-named symbol in this repository; the owning segment of
a two-segment path is still recorded as a type reference. The consequence is
an under-approximation: an associated call to a type this repository does own,
such as `Analyzer::default()`, is also left unresolved rather than guessed.

## Lossless guarantee

`weavatrix-parse` preserves every source byte in its token stream. Release
tests assert byte-for-byte round trips, malformed-input behavior, exact fact
spans, real-repository digests, GraphQL/Protobuf fixtures, and integration
through the repository engine.

New adapters must provide:

- malformed and incomplete input tests;
- exact facts and source spans;
- lossless round-trip evidence;
- real-repository regression fixtures;
- operation-level integration coverage.
