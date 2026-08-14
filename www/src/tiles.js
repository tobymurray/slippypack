// Web Mercator slippy-tile math. Mirrors
// slippypack-core::projection::mercator, which is the authority — these
// are here only so the page can lay out its render loop without a
// round trip into WASM per tile.

export const lon2x = (lon, z) => ((lon + 180) / 360) * 2 ** z;

export const lat2y = (lat, z) => {
  const r = (lat * Math.PI) / 180;
  return ((1 - Math.log(Math.tan(r) + 1 / Math.cos(r)) / Math.PI) / 2) * 2 ** z;
};

export const x2lon = (x, z) => (x / 2 ** z) * 360 - 180;

export const y2lat = (y, z) => {
  const n = Math.PI - (2 * Math.PI * y) / 2 ** z;
  return (180 / Math.PI) * Math.atan(0.5 * (Math.exp(n) - Math.exp(-n)));
};

/** SHA-256 of a string, as the Uint8Array the WASM builder wants. */
export async function sha256(text) {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return new Uint8Array(digest);
}
