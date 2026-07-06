#!/usr/bin/env node
/**
 * Probe handshake completion packets from a MySQL-compatible server.
 * Usage: node scripts/handshake-probe.mjs [host] [port]
 */
import net from 'node:net';

const host = process.argv[2] ?? '127.0.0.1';
const port = Number(process.argv[3] ?? 3307);

function hex(buf) {
  return [...buf].map((b) => b.toString(16).padStart(2, '0')).join(' ');
}

function readExact(socket, n) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let got = 0;
    const onData = (buf) => {
      chunks.push(buf);
      got += buf.length;
      if (got >= n) {
        socket.off('data', onData);
        socket.off('error', onError);
        resolve(Buffer.concat(chunks).subarray(0, n));
      }
    };
    const onError = reject;
    socket.on('data', onData);
    socket.on('error', onError);
  });
}

async function readPacket(socket) {
  const hdr = await readExact(socket, 4);
  const len = hdr[0] | (hdr[1] << 8) | (hdr[2] << 16);
  const seq = hdr[3];
  const body = len ? await readExact(socket, len) : Buffer.alloc(0);
  return { seq, body };
}

function writePacket(socket, seq, body) {
  const hdr = Buffer.alloc(4);
  hdr[0] = body.length & 0xff;
  hdr[1] = (body.length >> 8) & 0xff;
  hdr[2] = (body.length >> 16) & 0xff;
  hdr[3] = seq;
  socket.write(Buffer.concat([hdr, body]));
}

/** Caps similar to mysql 8.0.46 client with --ssl-mode=DISABLED */
const CLIENT_CAPS =
  0x00000200 | // PROTOCOL_41
  0x00008000 | // SECURE_CONNECTION
  0x00080000 | // PLUGIN_AUTH
  0x00200000 | // PLUGIN_AUTH_LENENC
  0x00020000 | // MULTI_RESULTS
  0x00040000 | // PS_MULTI_RESULTS
  0x00100000 | // CONNECT_ATTRS
  0x00800000 | // SESSION_TRACK
  0x01000000 | // DEPRECATE_EOF
  0x08000000; // QUERY_ATTRIBUTES

function buildHandshakeResponse(scramble, plugin) {
  const user = 'root';
  const auth = Buffer.alloc(32, 0xab); // non-empty like real caching_sha2 client
  const parts = [
    Buffer.from([
      CLIENT_CAPS & 0xff,
      (CLIENT_CAPS >> 8) & 0xff,
      (CLIENT_CAPS >> 16) & 0xff,
      (CLIENT_CAPS >> 24) & 0xff,
    ]),
    Buffer.from([255, 255, 255, 255]),
    Buffer.from([255]),
    Buffer.alloc(23),
    Buffer.from(`${user}\0`),
  ];
  const body = Buffer.concat(parts);
  const out = Buffer.alloc(body.length + 1 + 32 + plugin.length + 1 + 50);
  let o = 0;
  body.copy(out, o);
  o += body.length;
  out[o++] = 32;
  auth.copy(out, o);
  o += 32;
  out.write(`${plugin}\0`, o);
  o += plugin.length + 1;
  // connect attrs: lenenc blob
  const attrs = Buffer.from([
    0x0c, 0x0c, 0x5f, 0x63, 0x6c, 0x69, 0x65, 0x6e, 0x74, 0x5f, 0x6e, 0x61, 0x6d, 0x65, 0x06, 0x6c,
    0x69, 0x62, 0x6d, 0x79, 0x73, 0x71, 0x6c,
  ]);
  attrs.copy(out, o);
  o += attrs.length;
  return out.subarray(0, o);
}

const socket = net.createConnection({ host, port }, async () => {
  try {
    const hs = await readPacket(socket);
    console.log(`# ${host}:${port}`);
    console.log(`S->C hs seq=${hs.seq} len=${hs.body.length}`);
    const nul = hs.body.indexOf(0, 1);
    const ver = hs.body.subarray(1, nul).toString();
    const pluginStart = hs.body.lastIndexOf(0, hs.body.length - 2) + 1;
    let pluginEnd = pluginStart;
    while (pluginEnd < hs.body.length && hs.body[pluginEnd] !== 0) pluginEnd++;
    const plugin = hs.body.subarray(pluginStart, pluginEnd).toString();
    console.log(`  version=${ver} plugin=${plugin}`);

    const resp = buildHandshakeResponse(null, plugin);
    writePacket(socket, 1, resp);

    for (let i = 0; i < 5; i++) {
      const pkt = await readPacket(socket);
      console.log(`S->C seq=${pkt.seq} len=${pkt.body.length} tag=0x${pkt.body[0]?.toString(16)}`);
      console.log(`  ${hex(pkt.body)}`);
      if (pkt.body[0] === 0x00 && pkt.body.length > 2) break;
      if (pkt.body[0] === 0xff) break;
    }
    socket.end();
    process.exit(0);
  } catch (e) {
    console.error(e);
    socket.end();
    process.exit(1);
  }
});

socket.setTimeout(8000, () => {
  console.error('timeout');
  socket.end();
  process.exit(1);
});
