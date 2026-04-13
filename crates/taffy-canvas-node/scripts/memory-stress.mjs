import assert from "node:assert/strict";
import { createRequire } from "node:module";
import path from "node:path";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const require = createRequire(__filename);
const binding = require(path.join(__dirname, "..", "index.js"));

if (typeof global.gc !== "function") {
  throw new Error("Run this script with --expose-gc.");
}

const GRID_COLUMNS = 16;
const SPRITE_COUNT = 128;
const IMAGE_ITERATIONS = 360;
const TEXT_ITERATIONS = 720;
const CHECKPOINT_INTERVAL = 90;
const IMAGE_MAX_POST_WARMUP_RSS_GROWTH_BYTES = 20 * 1024 * 1024;
const TEXT_MAX_POST_WARMUP_RSS_GROWTH_BYTES = 20 * 1024 * 1024;
const INCLUDE_RAW_DIAGNOSTIC = process.env.TAFFY_STRESS_INCLUDE_RAW === "1";

const crcTable = new Uint32Array(256).map((_, seed) => {
  let value = seed;
  for (let bit = 0; bit < 8; bit += 1) {
    value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  }
  return value >>> 0;
});

function crc32(buffer) {
  let value = 0xffffffff;
  for (const byte of buffer) {
    value = crcTable[(value ^ byte) & 0xff] ^ (value >>> 8);
  }
  return (value ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const typeBuffer = Buffer.from(type);
  const lengthBuffer = Buffer.alloc(4);
  lengthBuffer.writeUInt32BE(data.length, 0);

  const chunkBuffer = Buffer.concat([typeBuffer, data]);
  const checksum = Buffer.alloc(4);
  checksum.writeUInt32BE(crc32(chunkBuffer), 0);
  return Buffer.concat([lengthBuffer, chunkBuffer, checksum]);
}

function createSolidPng(width, height, r, g, b) {
  const stride = width * 4 + 1;
  const pixels = Buffer.alloc(stride * height);

  for (let y = 0; y < height; y += 1) {
    const rowOffset = y * stride;
    pixels[rowOffset] = 0;
    for (let x = 0; x < width; x += 1) {
      const offset = rowOffset + 1 + x * 4;
      pixels[offset] = r;
      pixels[offset + 1] = g;
      pixels[offset + 2] = b;
      pixels[offset + 3] = 255;
    }
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", zlib.deflateSync(pixels, { level: 6 })),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

function forceGc() {
  for (let gcRun = 0; gcRun < 4; gcRun += 1) {
    global.gc();
  }
}

function collectCheckpoint(label, resources = null) {
  forceGc();

  const usage = process.memoryUsage();
  const stats = resources ? binding.inspectResources(resources) : null;
  return {
    label,
    rss: usage.rss,
    heapUsed: usage.heapUsed,
    external: usage.external,
    decodedImages: stats?.decodedImages ?? 0,
    preparedImages: stats?.preparedImages ?? 0,
    assets: stats?.assets ?? 0,
  };
}

function buildImageTemplateXml() {
  let xml = '<view width="1024" height="512" background="#000000">';
  for (let index = 0; index < SPRITE_COUNT; index += 1) {
    const x = (index % GRID_COLUMNS) * 64;
    const y = Math.floor(index / GRID_COLUMNS) * 64;
    xml += `<image src="img${index}" width="64" height="64" left="${x}" top="${y}" position="absolute" fit="fill" />`;
  }
  xml += "</view>";
  return xml;
}

function buildTextTemplateXml() {
  return `
    <view width="640" height="360" background="#0b1020">
      <text left="24" top="24" width="592" font-size="28" color="#ffffff">{{title}}</text>
      <text left="24" top="64" width="592" font-size="18" color="#cbd5e1">{{subtitle}}</text>
      <text left="24" top="110" width="592" font-size="16" color="#f8fafc">{{body}}</text>
      <text left="24" top="210" width="592" font-size="16" color="#f8fafc">{{moves}}</text>
      <text left="24" top="310" width="592" font-size="14" color="#94a3b8">Turn {{turn}}</text>
    </view>
  `;
}

function textParams(seed) {
  return {
    title: `Battle HUD ${seed} ${"A".repeat(seed % 37)}`,
    subtitle: `Weather ${seed % 7} status ${(seed * 13) % 97}`,
    body: `Player ${seed} versus enemy ${(seed * 17) % 151} with HP ${(seed * 19) % 300}/${300 + (seed % 50)} and terrain ${seed % 5}.`,
    moves: Array.from(
      { length: 4 },
      (_, index) => `Move ${index + 1}-${seed}-${"x".repeat((seed + index) % 23)}`,
    ).join(" | "),
    turn: String(seed),
  };
}

function formatMiB(bytes) {
  return `${(bytes / 1024 / 1024).toFixed(2)} MiB`;
}

function printCheckpoint(scenario, checkpoint) {
  console.log(
    JSON.stringify({
      scenario,
      ...checkpoint,
      rss: formatMiB(checkpoint.rss),
      heapUsed: formatMiB(checkpoint.heapUsed),
      external: formatMiB(checkpoint.external),
    }),
  );
}

function assertPostWarmupBound(scenario, checkpoints, limitBytes) {
  const warmCheckpoint = checkpoints.find((checkpoint) => checkpoint.label === "180");
  const finalCheckpoint = checkpoints[checkpoints.length - 1];
  assert.ok(warmCheckpoint, `${scenario}: expected a warmup checkpoint at 180 renders`);

  const postWarmupGrowth = finalCheckpoint.rss - warmCheckpoint.rss;
  assert.ok(
    postWarmupGrowth <= limitBytes,
    `${scenario}: expected RSS growth after warmup to stay under ${formatMiB(limitBytes)}, got ${formatMiB(postWarmupGrowth)}`,
  );

  console.log(
    `${scenario}: memory stress passed with post-warmup rss growth ${formatMiB(postWarmupGrowth)}`,
  );
}

function runImageStress() {
  const baseResources = binding.createResources();
  const dynamicResources = binding.createResources();
  const template = binding.compileTemplate(buildImageTemplateXml());
  const prepared = binding.prepareTemplate(baseResources, template);
  const session = binding.createTemplateSession(prepared, {});
  const checkpoints = [collectCheckpoint("start", dynamicResources)];

  for (let iteration = 1; iteration <= IMAGE_ITERATIONS; iteration += 1) {
    for (let index = 0; index < SPRITE_COUNT; index += 1) {
      const seed = (iteration * 1315423911 + index * 2654435761) >>> 0;
      binding.addResourceAsset(
        dynamicResources,
        `img${index}`,
        createSolidPng(
          64,
          64,
          seed & 0xff,
          (seed >>> 8) & 0xff,
          (seed >>> 16) & 0xff,
        ),
      );
    }

    const buffer = binding.renderTemplateSessionWithResourcesSync(
      session,
      dynamicResources,
      {},
      "cpu",
    );
    assert.ok(Buffer.isBuffer(buffer));
    assert.ok(buffer.length > 0);

    if (iteration % CHECKPOINT_INTERVAL === 0) {
      checkpoints.push(collectCheckpoint(String(iteration), dynamicResources));
    }
  }

  for (const checkpoint of checkpoints) {
    printCheckpoint("image-grid-png", checkpoint);
  }

  for (const checkpoint of checkpoints.filter((entry) => entry.label !== "start")) {
    assert.equal(checkpoint.assets, SPRITE_COUNT, "dynamic asset slots should plateau");
    assert.equal(
      checkpoint.decodedImages,
      SPRITE_COUNT,
      "decoded image cache should plateau",
    );
    assert.equal(
      checkpoint.preparedImages,
      SPRITE_COUNT,
      "prepared image cache should plateau",
    );
  }

  assertPostWarmupBound(
    "image-grid-png",
    checkpoints,
    IMAGE_MAX_POST_WARMUP_RSS_GROWTH_BYTES,
  );
}

function runTextStress(outputFormat) {
  const template = binding.compileTemplate(buildTextTemplateXml());
  const resources = binding.createResources();
  const prepared = binding.prepareTemplate(resources, template);
  const session = binding.createTemplateSession(prepared, {});
  const checkpoints = [collectCheckpoint("start")];

  for (let iteration = 1; iteration <= TEXT_ITERATIONS; iteration += 1) {
    const seed = ((iteration - 1) % 180) + 1;
    const output = binding.renderTemplateSessionSync(
      session,
      textParams(seed),
      {
        backend: "cpu",
        outputFormat,
        outputSize: "fast",
      },
    );
    assert.ok(Buffer.isBuffer(output));
    assert.ok(output.length > 0);

    if (iteration % CHECKPOINT_INTERVAL === 0) {
      checkpoints.push(collectCheckpoint(String(iteration)));
    }
  }

  const scenario = `text-heavy-${outputFormat}`;
  for (const checkpoint of checkpoints) {
    printCheckpoint(scenario, checkpoint);
  }

  if (outputFormat === "png") {
    assertPostWarmupBound(
      scenario,
      checkpoints,
      TEXT_MAX_POST_WARMUP_RSS_GROWTH_BYTES,
    );
  }
}

runImageStress();
runTextStress("png");
if (INCLUDE_RAW_DIAGNOSTIC) {
  runTextStress("raw");
}
