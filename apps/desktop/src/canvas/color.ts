import type { Hex } from "../engine/types";

export type RGBA = [number, number, number, number];

const h2 = (n: number) => n.toString(16).padStart(2, "0");

export function toHex([r, g, b, a]: RGBA): Hex {
  return `#${h2(r)}${h2(g)}${h2(b)}${h2(a)}`;
}

/** Parses #rgb, #rrggbb and #rrggbbaa. Returns null for anything else. */
export function parseHex(hex: string): RGBA | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(hex.trim());
  if (!m) return null;
  let s = m[1]!;
  if (s.length === 3) s = [...s].map((c) => c + c).join("");
  if (s.length === 6) s += "ff";
  const n = (i: number) => parseInt(s.slice(i, i + 2), 16);
  return [n(0), n(2), n(4), n(6)];
}

export function rgba(hex: Hex): RGBA {
  return parseHex(hex) ?? [0, 0, 0, 0];
}

export function normalizeHex(hex: string): Hex | null {
  const c = parseHex(hex);
  return c ? toHex(c) : null;
}

export const TRANSPARENT: Hex = "#00000000";

/** CSS color for a hex value (CSS understands #rrggbbaa). */
export const css = (hex: Hex) => hex;

/**
 * Integer source-over, straight alpha. Exact port of
 * `dotify_core::composite::blend_over`.
 */
export function blendOver(dst: RGBA, src: RGBA, opacity: number): RGBA {
  const sa = Math.floor((src[3] * opacity + 127) / 255);
  if (sa === 0) return dst;
  if (sa === 255) return [src[0], src[1], src[2], 255];
  const da = dst[3];
  const dstWeight = da * (255 - sa);
  const outA = sa + Math.floor((dstWeight + 127) / 255);
  const denom = outA * 255;
  const out: RGBA = [0, 0, 0, Math.min(outA, 255)];
  for (let c = 0; c < 3; c++) {
    const num = src[c]! * sa * 255 + dst[c]! * dstWeight;
    out[c] = Math.min(Math.floor((num + Math.floor(denom / 2)) / denom), 255);
  }
  return out;
}

export interface HSVA {
  h: number; // 0–360
  s: number; // 0–1
  v: number; // 0–1
  a: number; // 0–255
}

export function rgbaToHsva([r, g, b, a]: RGBA): HSVA {
  const rn = r / 255,
    gn = g / 255,
    bn = b / 255;
  const max = Math.max(rn, gn, bn),
    min = Math.min(rn, gn, bn);
  const d = max - min;
  let h = 0;
  if (d !== 0) {
    if (max === rn) h = ((gn - bn) / d) % 6;
    else if (max === gn) h = (bn - rn) / d + 2;
    else h = (rn - gn) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  return { h, s: max === 0 ? 0 : d / max, v: max, a };
}

export function hsvaToRgba({ h, s, v, a }: HSVA): RGBA {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  const [r, g, b] =
    h < 60
      ? [c, x, 0]
      : h < 120
        ? [x, c, 0]
        : h < 180
          ? [0, c, x]
          : h < 240
            ? [0, x, c]
            : h < 300
              ? [x, 0, c]
              : [c, 0, x];
  return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255), a];
}
