# Languages and repository surfaces

Weavatrix combines the lossless `weavatrix-parse` pipeline with
language-specific adapters and domain extractors.

| Surface | Extracted evidence |
| --- | --- |
| Rust | modules, uses/re-exports, items, impl owners, traits, calls, tests, routes |
| JavaScript / TypeScript | modules, imports/exports, declarations, members, calls, routes |
| Python | imports, declarations, classes, calls, framework evidence |
| Go | packages, imports, declarations, receivers, calls, HTTP/gRPC |
| Java | packages, classes, methods, inheritance, Spring and messaging |
| C# | namespaces, types, members, calls, routes |
| C / C++ | includes, declarations, functions, calls |
| SQL | schemas, tables, fields, statements, host references |
| Bash, Solidity, Swift | structural declarations and calls |

The parser also preserves HTML, CSS/SCSS/Less, Terraform, XML, Markdown, MDX,
RST, and AsciiDoc with byte-for-byte source recovery.

## Contracts and infrastructure

- HTTP routes and mount chains;
- GraphQL operations and definitions;
- Protobuf messages and gRPC services/streaming modes;
- Kafka, RabbitMQ/AMQP, JMS, NATS, SQS, and SNS identities;
- MongoDB and package/configuration evidence;
- Kubernetes YAML, JSON/JSONC, manifests, and lockfiles.

## Precision boundary

Syntax and manifest facts are deterministic. Call resolution uses imports,
ownership, scope, module paths, and exact spans. Dynamic dispatch that cannot
be proven stays unresolved with evidence rather than being connected to a
same-named function.

## Lossless guarantee

`weavatrix-parse` keeps every source byte in its token stream. Release tests
assert round-trip recovery and exact fact digests over real repositories, plus
GraphQL/Protobuf fixtures and import agreement against tree-sitter.

New adapters must include malformed-input tests, exact facts/spans,
real-repository regression evidence, and tool-level integration tests.
