// Reading of the export format (mirror of crates/wa-export/src/format.rs).
import { argon2id } from "hash-wasm";
import { ShellError } from "./messages";

export const SEGMENT = 1 << 20;
export const TAG = 16;
const enc = new TextEncoder();

export function b64(s: string): Uint8Array<ArrayBuffer> {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export type Kdf = { alg: string; m: number; t: number; p: number; salt: string };

/** Password → slot key (Argon2id in WASM, ~1 s with the default parameters). */
export async function passwordKey(password: string, kdf: Kdf): Promise<CryptoKey> {
  if (kdf.alg !== "argon2id") throw new ShellError("unknown_kdf");
  const raw = await argon2id({
    password,
    salt: b64(kdf.salt),
    parallelism: kdf.p,
    iterations: kdf.t,
    memorySize: kdf.m,
    hashLength: 32,
    outputType: "binary",
  });
  const key = await crypto.subtle.importKey("raw", new Uint8Array(raw), "AES-GCM", false, ["decrypt"]);
  raw.fill(0);
  return key;
}

export async function aesKey(raw: Uint8Array<ArrayBuffer>): Promise<CryptoKey> {
  return crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["decrypt"]);
}

export async function open(
  key: CryptoKey,
  nonce: Uint8Array<ArrayBuffer>,
  ct: Uint8Array<ArrayBuffer>,
  aad: string | Uint8Array<ArrayBuffer>,
): Promise<Uint8Array<ArrayBuffer>> {
  const additionalData = typeof aad === "string" ? enc.encode(aad) : aad;
  return new Uint8Array(await crypto.subtle.decrypt({ name: "AES-GCM", iv: nonce, additionalData }, key, ct));
}

/** HKDF-SHA256 → 32 bytes. */
export async function hkdf(ikm: Uint8Array<ArrayBuffer>, salt: Uint8Array<ArrayBuffer>, info: string): Promise<Uint8Array<ArrayBuffer>> {
  const base = await crypto.subtle.importKey("raw", ikm, "HKDF", false, ["deriveBits"]);
  const bits = await crypto.subtle.deriveBits({ name: "HKDF", hash: "SHA-256", salt, info: enc.encode(info) }, base, 256);
  return new Uint8Array(bits);
}

/** `[u32 LE length][gzip][padding]` → JSON. */
export async function unpackChunk(plain: Uint8Array<ArrayBuffer>): Promise<unknown> {
  const len = new DataView(plain.buffer, plain.byteOffset).getUint32(0, true);
  const gz = plain.subarray(4, 4 + len);
  const stream = new Blob([gz]).stream().pipeThrough(new DecompressionStream("gzip"));
  const text = await new Response(stream).text();
  plain.fill(0);
  return JSON.parse(text);
}

export function streamLen(size: number): number {
  const segs = Math.max(1, Math.ceil(size / SEGMENT));
  return size + segs * TAG;
}

/** Decrypts a STREAM media file (1 MiB segments) read from a bundle. */
export async function openStream(
  key: CryptoKey,
  prefix: Uint8Array<ArrayBuffer>,
  mediaId: string,
  read: (offset: number, length: number) => Promise<Uint8Array<ArrayBuffer>>,
  size: number,
): Promise<Uint8Array<ArrayBuffer>[]> {
  const segs = Math.max(1, Math.ceil(size / SEGMENT));
  const out: Uint8Array<ArrayBuffer>[] = [];
  const aad = enc.encode(mediaId);
  let off = 0;
  for (let i = 0; i < segs; i++) {
    const plainLen = Math.min(SEGMENT, size - i * SEGMENT);
    const ct = await read(off, plainLen + TAG);
    const nonce = new Uint8Array(12);
    nonce.set(prefix, 0);
    new DataView(nonce.buffer).setUint32(7, i, false);
    nonce[11] = i + 1 === segs ? 1 : 0;
    out.push(await open(key, nonce, ct, aad));
    off += plainLen + TAG;
  }
  return out;
}
