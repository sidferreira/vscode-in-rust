//! Extension host RPC in Rust (ADR-0020 Option A, ADR-0060): the main-thread side of the wire that
//! upstream's own `extensionHostProcess` speaks. UI-free and free of async runtimes: the protocol
//! pieces are sans-IO state machines, and [`handshake`] drives them over a Unix socket.
//!
//! upstream: src/vs/base/parts/ipc/common/ipc.net.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/extensions/common/rpcProtocol.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/extensions/common/extensionHostProtocol.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/extensions/common/extensionHostEnv.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/node/extensionHostProcess.ts @ 50e1350fc78d
//!
//! Layers, bottom to top:
//!
//! * [`framing`]: the 13-byte-header frames and the acknowledge / replay bookkeeping of
//!   `PersistentProtocol`;
//! * [`codec`]: `MessageBuffer` / `MessageIO` of `rpcProtocol.ts`: requests, replies, errors, acks;
//! * [`rpc`]: request ids and dispatch to a [`rpc::Handler`] (the `RPCProtocol` class);
//! * [`proxy_ids`]: the `MainContext` / `ExtHostContext` numbering, generated from
//!   `extHost.protocol.ts` by `scripts/gen-rpc-ids`;
//! * [`session`]: a long-lived connection: reader thread, events channel, async replies (ADR-0063);
//! * [`handshake`]: spawn the host under Node and complete `Ready` -> init data -> `Initialized`.

pub mod codec;
pub mod framing;
pub mod handshake;
pub mod proxy_ids;
pub mod rpc;
pub mod session;
