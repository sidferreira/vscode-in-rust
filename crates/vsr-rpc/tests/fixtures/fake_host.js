// upstream: src/vs/workbench/api/node/extensionHostProcess.ts @ 50e1350fc78d
// A stand-in for upstream's extensionHostProcess, written independently of the Rust code in plain
// Node (no dependencies), to test the handshake where no Code-OSS build is available.
// It connects to VSCODE_EXTHOST_IPC_HOOK and speaks ipc.net.ts framing + rpcProtocol.ts messages:
//   Resume, Ready -> expects init data JSON -> Initialized -> request MainThreadCommands.$registerCommand
//   -> expects Acknowledged + ReplyOKEmpty -> waits for Terminate and exits 0 (2 on any mismatch).
const net = require('net');
const sock = net.createConnection(process.env.VSCODE_EXTHOST_IPC_HOOK);
let buf = Buffer.alloc(0), outId = 0, inId = 0, stage = 0;
const fail = (m) => { console.error('fake host: ' + m); process.exit(2); };
const frame = (type, id, ack, data) => {
  const h = Buffer.alloc(13);
  h[0] = type; h.writeUInt32BE(id, 1); h.writeUInt32BE(ack, 5); h.writeUInt32BE(data.length, 9);
  return Buffer.concat([h, data]);
};
const send = (data) => { outId++; sock.write(frame(1, outId, inId, data)); };
sock.on('connect', () => { sock.write(frame(8, 0, 0, Buffer.alloc(0))); send(Buffer.from([2])); });
sock.on('data', (d) => {
  buf = Buffer.concat([buf, d]);
  while (buf.length >= 13) {
    const len = buf.readUInt32BE(9);
    if (buf.length < 13 + len) return;
    const type = buf[0], id = buf.readUInt32BE(1), body = buf.subarray(13, 13 + len);
    buf = buf.subarray(13 + len);
    if (type !== 1) continue; // only regular messages matter here
    if (id !== inId + 1) fail('gap in ids');
    inId = id;
    if (stage === 0) {
      const init = JSON.parse(body.toString());
      if (init.version !== '1.147.0' || init.environment.appName !== 'rscode') fail('bad init data');
      send(Buffer.from([1]));
      // JSON-args request: type 1, req 1, rpcId 10, method, args
      const method = Buffer.from('$registerCommand'), args = Buffer.from('["fake.cmd"]');
      const head = Buffer.alloc(6); head[0] = 1; head.writeUInt32BE(1, 1); head[5] = Number(process.env.FAKE_RPC_ID || 10);
      const al = Buffer.alloc(4); al.writeUInt32BE(args.length);
      send(Buffer.concat([head, Buffer.from([method.length]), method, al, args]));
      stage = 1;
    } else if (stage === 1) {
      if (body[0] !== 5 || body.readUInt32BE(1) !== 1) fail('expected Acknowledged for req 1');
      stage = 2;
    } else if (stage === 2) {
      if (body[0] !== 7 || body.readUInt32BE(1) !== 1) fail('expected ReplyOKEmpty for req 1');
      stage = 3;
    } else if (stage === 3) {
      if (body.length === 1 && body[0] === 3) process.exit(0);
      fail('expected Terminate');
    }
  }
});
sock.on('error', (e) => fail(String(e)));
