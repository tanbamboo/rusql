/**
 * Minimal persistent MySQL wire client for rusql/MySQL benchmarks (PERF-B1).
 * Avoids mysql2 handshake quirks with rusql caching_sha2 dev mode.
 */
import net from 'node:net';

const COM_QUERY = 0x03;
const COM_QUIT = 0x01;

const CLIENT_CAPS =
  0x00000200 | // PROTOCOL_41
  0x00008000 | // SECURE_CONNECTION
  0x00080000 | // PLUGIN_AUTH
  0x00200000; // PLUGIN_AUTH_LENENC

export class WireBenchClient {
  #socket;
  #pending = Buffer.alloc(0);
  #waiters = [];
  #seq = 0;

  constructor(socket) {
    this.#socket = socket;
    socket.on('data', (chunk) => this.#onData(chunk));
  }

  static connect({ host, port, user = 'root', password = '' }) {
    return new Promise((resolve, reject) => {
      const socket = net.createConnection({ host, port }, async () => {
        try {
          const client = new WireBenchClient(socket);
          await client.#handshake(user, password);
          resolve(client);
        } catch (e) {
          socket.destroy();
          reject(e);
        }
      });
      socket.on('error', reject);
      socket.setTimeout(30_000, () => {
        socket.destroy(new Error('connect timeout'));
      });
    });
  }

  #onData(chunk) {
    this.#pending = Buffer.concat([this.#pending, chunk]);
    while (this.#waiters.length && this.#pending.length >= this.#waiters[0].need) {
      const { need, resolve } = this.#waiters.shift();
      resolve(this.#pending.subarray(0, need));
      this.#pending = this.#pending.subarray(need);
    }
  }

  #readExact(n) {
    if (this.#pending.length >= n) {
      const out = this.#pending.subarray(0, n);
      this.#pending = this.#pending.subarray(n);
      return Promise.resolve(out);
    }
    return new Promise((resolve, reject) => {
      this.#waiters.push({ need: n, resolve, reject });
    });
  }

  async #readPacket() {
    const hdr = await this.#readExact(4);
    const len = hdr[0] | (hdr[1] << 8) | (hdr[2] << 16);
    const seq = hdr[3];
    const body = len ? await this.#readExact(len) : Buffer.alloc(0);
    return { seq, body };
  }

  #writePacket(body) {
    const hdr = Buffer.alloc(4);
    hdr[0] = body.length & 0xff;
    hdr[1] = (body.length >> 8) & 0xff;
    hdr[2] = (body.length >> 16) & 0xff;
    hdr[3] = this.#seq++ & 0xff;
    this.#socket.write(Buffer.concat([hdr, body]));
  }

  async #handshake(user, password) {
    const hs = await this.#readPacket();
    this.#seq = 1;
    const resp = buildHandshakeResponse(user, password, hs.body);
    this.#writePacket(resp);

    for (let i = 0; i < 6; i++) {
      const pkt = await this.#readPacket();
      if (pkt.body[0] === 0x00) {
        this.#seq = 0;
        return;
      }
      if (pkt.body[0] === 0xff) {
        throw new Error(`handshake ERR: ${pkt.body.toString('utf8', 3)}`);
      }
      if (pkt.body[0] === 0x01 && pkt.body[1] === 0x03) continue;
      if (pkt.body[0] === 0x01 && pkt.body[1] === 0x04) continue;
    }
    throw new Error('handshake did not complete');
  }

  async query(sql) {
    const cmd = Buffer.concat([Buffer.from([COM_QUERY]), Buffer.from(sql, 'utf8')]);
    this.#writePacket(cmd);
    await drainQueryResponse(this.#readPacket.bind(this));
  }

  async end() {
    this.#writePacket(Buffer.from([COM_QUIT]));
    this.#socket.end();
  }
}

function buildHandshakeResponse(user, password, hsBody) {
  const plugin = readNullString(hsBody, hsBody.lastIndexOf(0, hsBody.length - 2) + 1);
  const auth = password
    ? Buffer.from(password, 'utf8')
    : Buffer.alloc(0);
  const parts = [
    uint32Le(CLIENT_CAPS),
    Buffer.from([255, 255, 255, 255]),
    Buffer.from([45]),
    Buffer.alloc(23),
    Buffer.from(`${user}\0`),
    Buffer.from([auth.length]),
    auth,
    Buffer.from(`${plugin}\0`),
  ];
  return Buffer.concat(parts);
}

function readNullString(buf, start) {
  let end = start;
  while (end < buf.length && buf[end] !== 0) end++;
  return buf.subarray(start, end).toString('utf8');
}

function uint32Le(n) {
  return Buffer.from([n & 0xff, (n >> 8) & 0xff, (n >> 16) & 0xff, (n >> 24) & 0xff]);
}

function isTerminator(body) {
  if (body.length === 0) return true;
  if (body[0] === 0x00) return true;
  if (body[0] === 0xfe && body.length < 9) return true;
  return false;
}

async function drainQueryResponse(readPacket) {
  const first = await readPacket();
  if (first.body[0] === 0x00 || first.body[0] === 0xff) return first.body;
  const colCount = readLenencInt(first.body, 0);
  for (let i = 0; i < colCount; i++) {
    await readPacket();
  }
  const afterCols = await readPacket();
  if (afterCols.body[0] === 0xff) return afterCols.body;
  if (colCount === 0) return afterCols.body;
  while (true) {
    const pkt = await readPacket();
    if (isTerminator(pkt.body)) return pkt.body;
  }
}

function readLenencInt(buf, pos) {
  const first = buf[pos];
  if (first < 0xfb) return first;
  if (first === 0xfc) return buf[pos + 1] | (buf[pos + 2] << 8);
  return 0;
}
