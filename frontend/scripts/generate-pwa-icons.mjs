/**
 * Generates placeholder PWA icons (brand-blue card glyph) into public/.
 *
 * Dependency-free: draws shapes at 4x supersampling for smooth edges, then
 * box-downsamples and encodes PNG via Node's built-in zlib. Re-run after
 * changing the design or to replace with a real logo source.
 *
 *   node scripts/generate-pwa-icons.mjs
 */
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const OUTPUT_DIR = join(dirname(fileURLToPath(import.meta.url)), "..", "public");

// Brand palette (matches --primary hsl(217 91% 56%)).
const BRAND_BLUE = [41, 119, 245];
const WHITE = [255, 255, 255];
const LIGHT_BLUE = [147, 197, 253];

const isInsideRoundedRect = (px, py, x, y, width, height, radius) => {
  if (px < x || px >= x + width || py < y || py >= y + height) return false;
  const nearestX = Math.max(x + radius, Math.min(px, x + width - radius));
  const nearestY = Math.max(y + radius, Math.min(py, y + height - radius));
  const dx = px - nearestX;
  const dy = py - nearestY;
  return dx * dx + dy * dy <= radius * radius;
};

const isInsideCircle = (px, py, cx, cy, radius) => {
  const dx = px - cx;
  const dy = py - cy;
  return dx * dx + dy * dy <= radius * radius;
};

/**
 * Render the icon at full resolution with hard edges.
 *
 * @param size - Supersampled canvas size in pixels (square)
 * @returns RGBA pixel buffer of length size*size*4
 */
const renderSupersampled = (size) => {
  const pixels = Buffer.alloc(size * size * 4);

  const cardWidth = size * 0.5;
  const cardHeight = size * 0.36;
  const cardX = (size - cardWidth) / 2;
  const cardY = (size - cardHeight) / 2;
  const cardRadius = size * 0.06;

  const dividerY = cardY + cardHeight * 0.3;
  const dividerHeight = size * 0.022;
  const dividerPad = cardWidth * 0.12;

  const claspRadius = size * 0.055;
  const claspX = cardX + cardWidth - cardWidth * 0.2;
  const claspY = cardY + cardHeight * 0.66;

  for (let py = 0; py < size; py++) {
    for (let px = 0; px < size; px++) {
      let color = BRAND_BLUE; // full-bleed background (maskable-safe)

      if (isInsideRoundedRect(px, py, cardX, cardY, cardWidth, cardHeight, cardRadius)) {
        color = WHITE;
        if (
          px >= cardX + dividerPad &&
          px <= cardX + cardWidth - dividerPad &&
          py >= dividerY &&
          py < dividerY + dividerHeight
        ) {
          color = LIGHT_BLUE;
        }
        if (isInsideCircle(px, py, claspX, claspY, claspRadius)) {
          color = BRAND_BLUE;
        }
      }

      const offset = (py * size + px) * 4;
      pixels[offset] = color[0];
      pixels[offset + 1] = color[1];
      pixels[offset + 2] = color[2];
      pixels[offset + 3] = 255;
    }
  }
  return pixels;
};

/**
 * Box-downsample a supersampled RGBA buffer to the target size.
 *
 * @param source - RGBA buffer at sourceSize resolution
 * @param sourceSize - Side length of the source buffer
 * @param targetSize - Desired output side length (must divide sourceSize)
 * @returns RGBA buffer at targetSize resolution
 */
const downsample = (source, sourceSize, targetSize) => {
  const factor = sourceSize / targetSize;
  const output = Buffer.alloc(targetSize * targetSize * 4);
  const samples = factor * factor;

  for (let ty = 0; ty < targetSize; ty++) {
    for (let tx = 0; tx < targetSize; tx++) {
      let sumR = 0;
      let sumG = 0;
      let sumB = 0;
      for (let sy = 0; sy < factor; sy++) {
        for (let sx = 0; sx < factor; sx++) {
          const srcOffset = ((ty * factor + sy) * sourceSize + (tx * factor + sx)) * 4;
          sumR += source[srcOffset];
          sumG += source[srcOffset + 1];
          sumB += source[srcOffset + 2];
        }
      }
      const dstOffset = (ty * targetSize + tx) * 4;
      output[dstOffset] = Math.round(sumR / samples);
      output[dstOffset + 1] = Math.round(sumG / samples);
      output[dstOffset + 2] = Math.round(sumB / samples);
      output[dstOffset + 3] = 255;
    }
  }
  return output;
};

// --- Minimal PNG encoder (RGBA, 8-bit, no interlace) ---

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let crc = n;
    for (let k = 0; k < 8; k++) {
      crc = crc & 1 ? 0xedb88320 ^ (crc >>> 1) : crc >>> 1;
    }
    table[n] = crc >>> 0;
  }
  return table;
})();

const crc32 = (buffer) => {
  let crc = 0xffffffff;
  for (let i = 0; i < buffer.length; i++) {
    crc = CRC_TABLE[(crc ^ buffer[i]) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
};

const makeChunk = (type, data) => {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const typeBytes = Buffer.from(type, "ascii");
  const crcInput = Buffer.concat([typeBytes, data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(crcInput), 0);
  return Buffer.concat([length, typeBytes, data, crc]);
};

const encodePng = (rgba, size) => {
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type: RGBA
  ihdr[10] = 0; // compression
  ihdr[11] = 0; // filter
  ihdr[12] = 0; // interlace

  // Prefix each scanline with filter byte 0 (none).
  const stride = size * 4;
  const raw = Buffer.alloc((stride + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (stride + 1)] = 0;
    rgba.copy(raw, y * (stride + 1) + 1, y * stride, y * stride + stride);
  }

  return Buffer.concat([
    signature,
    makeChunk("IHDR", ihdr),
    makeChunk("IDAT", deflateSync(raw, { level: 9 })),
    makeChunk("IEND", Buffer.alloc(0)),
  ]);
};

const writeIcon = (filename, targetSize) => {
  const superSize = targetSize * 4;
  const supersampled = renderSupersampled(superSize);
  const downsampled = downsample(supersampled, superSize, targetSize);
  const png = encodePng(downsampled, targetSize);
  writeFileSync(join(OUTPUT_DIR, filename), png);
  console.log(`wrote public/${filename} (${png.length} bytes)`);
};

mkdirSync(OUTPUT_DIR, { recursive: true });
writeIcon("pwa-192x192.png", 192);
writeIcon("pwa-512x512.png", 512);
writeIcon("pwa-maskable-512x512.png", 512);
writeIcon("apple-touch-icon.png", 180);
