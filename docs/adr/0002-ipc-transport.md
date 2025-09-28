# ADR-0002: IPC Transport Layer

**Status:** Accepted  
**Date:** 2024-01-15  
**Authors:** Architecture Team  
**Reviewers:** Engineering Team  
**Related ADRs:** ADR-0001 (FFB Modes), ADR-0004 (Plugin Architecture)

## Context

The racing wheel software requires efficient inter-process communication between the real-time service (wheeld) and client applications (UI, CLI). The IPC layer must support:

1. Cross-platform operation (Windows, Linux)
2. Schema-first contracts with versioning
3. Streaming health events and device enumeration
4. Security through OS-level permissions
5. Low latency for control operations

## Decision

Implement gRPC over platform-specific transports with Protobuf contracts:

- **Windows**: Named Pipes (`\\.\pipe\wheel`) with ACL restrictions
- **Linux**: Unix Domain Sockets (`/run/user/<uid>/wheel.sock`) with file permissions
- **Protocol**: gRPC with Protobuf schemas for all service contracts
- **Versioning**: Feature negotiation within `wheel.v1` namespace

## Rationale

- **Schema-first**: Protobuf provides strong typing and backward compatibility
- **Cross-platform**: gRPC abstracts transport differences while allowing platform optimization
- **Security**: OS-level permissions provide process isolation
- **Performance**: Local transports avoid network overhead
- **Tooling**: Rich ecosystem for code generation and testing

## Consequences

### Positive
- Strong typing prevents protocol errors
- Automatic code generation from schemas
- Built-in versioning and compatibility checking
- Excellent tooling and debugging support
- Clear separation between transport and application logic

### Negative
- Additional complexity compared to simple JSON over sockets
- Protobuf serialization overhead (minimal for local IPC)
- Dependency on gRPC runtime

### Neutral
- Requires buf or similar tooling for schema management
- Transport layer abstraction may hide platform-specific optimizations

## Alternatives Considered

1. **JSON over TCP**: Rejected due to lack of schema enforcement and versioning
2. **MessagePack over UDS**: Rejected due to limited tooling and manual schema management
3. **Shared Memory**: Rejected due to complexity and cross-platform compatibility issues
4. **D-Bus (Linux only)**: Rejected due to Windows compatibility requirements

## Implementation Notes

- Service contracts defined in `schemas/` crate with buf-managed Protobuf files
- Transport abstraction allows testing with in-memory channels
- ACL setup handled during service installation
- Feature negotiation occurs on first RPC call
- Streaming RPCs used for health events and device enumeration

## Compliance & Verification

- Schema compatibility tests prevent breaking changes
- Integration tests with mock clients validate all service methods
- Security tests verify ACL restrictions work correctly
- Performance tests measure RPC latency under load
- Cross-platform tests ensure transport parity

## References

- Requirements: XPLAT-02, UX-02, PLUG-03
- Design Document: IPC and Client Communication
- Protobuf Style Guide: https://developers.google.com/protocol-buffers/docs/style