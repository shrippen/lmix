// Relay between an RTMP publisher and a server that reports the
// publisher's start and end of stream: one line `publish` when it sends
// its publish command, `data` once media follows (`DATA_BYTES` after
// publish; headers and metadata are far less), `unpublish` when it sends
// deleteStream (a clean end; a dropped connection sends neither).
// scripts/test-m2b-vm.sh.
//
//   node scripts/rtmp-unpublish.cjs LISTEN_PORT SERVER_PORT   # 127.0.0.1
//
//   publisher ─► :LISTEN_PORT ─► relay ─► :SERVER_PORT (ffmpeg -listen 1)
//                                  └─► stdout: publish / unpublish
//
// Command names travel as plain AMF0 strings (marker 0x02, 16-bit length,
// name); the tail of the last read is kept so a name split across two
// reads is still found.
"use strict";
const net = require("net");

const HOST = "127.0.0.1";
const [listenPort, serverPort] = process.argv.slice(2).map(Number);
const amf = (name) => Buffer.concat([Buffer.from([0x02, 0x00, name.length]), Buffer.from(name)]);
const MARKS = [[amf("publish"), "publish"], [amf("deleteStream"), "unpublish"]];
const WINDOW = Math.max(...MARKS.map(([m]) => m.length));
// A quarter second of a 2 Mbit/s stream.
const DATA_BYTES = 64 * 1024;

net.createServer((client) => {
  const server = net.connect(serverPort, HOST);
  const seen = new Set();
  let tail = Buffer.alloc(0);
  let sent = 0;
  client.on("data", (data) => {
    if (seen.has("publish") && !seen.has("data")) {
      sent += data.length;
      if (sent >= DATA_BYTES) {
        seen.add("data");
        console.log("data");
      }
    }
    // Each command once, in the order the publisher sent it.
    const window = Buffer.concat([tail, data]);
    for (const [mark, line] of MARKS) {
      if (!seen.has(line) && window.includes(mark)) {
        seen.add(line);
        console.log(line);
      }
    }
    tail = window.subarray(-WINDOW);
  });
  client.pipe(server);
  server.pipe(client);
  client.on("error", () => server.destroy());
  server.on("error", () => client.destroy());
}).listen(listenPort, HOST);
