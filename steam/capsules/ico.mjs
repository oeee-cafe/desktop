// Packs PNGs into one ICO, each kept as PNG inside it (as Windows has read
// since Vista), in the order given. The size of each is read from its header.
//
//   node ico.mjs out.ico 16.png 32.png ... 256.png

import { readFileSync, writeFileSync } from "node:fs";

const [out, ...inputs] = process.argv.slice(2);
const images = inputs.map((file) => {
  const data = readFileSync(file);
  return { data, width: data.readUInt32BE(16), height: data.readUInt32BE(20) };
});

const header = Buffer.alloc(6);
header.writeUInt16LE(0, 0); // reserved
header.writeUInt16LE(1, 2); // icon
header.writeUInt16LE(images.length, 4);

let offset = 6 + 16 * images.length;
const entries = images.map(({ data, width, height }) => {
  const entry = Buffer.alloc(16);
  entry.writeUInt8(width >= 256 ? 0 : width, 0); // 0 means 256
  entry.writeUInt8(height >= 256 ? 0 : height, 1);
  entry.writeUInt8(0, 2); // no palette
  entry.writeUInt8(0, 3);
  entry.writeUInt16LE(1, 4); // colour planes
  entry.writeUInt16LE(32, 6); // bits per pixel
  entry.writeUInt32LE(data.length, 8);
  entry.writeUInt32LE(offset, 12);
  offset += data.length;
  return entry;
});

writeFileSync(out, Buffer.concat([header, ...entries, ...images.map((i) => i.data)]));
