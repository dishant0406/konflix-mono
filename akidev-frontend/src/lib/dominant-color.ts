// The colour a cover is mostly made of, for the tint behind an album or artist page.
//
// The cover is drawn small on a canvas and its pixels averaged, weighting the colourful ones so a
// mostly-black cover with a red logo gives red rather than grey. The picture comes from the API's
// image route, which answers cross-origin requests, so the canvas stays readable.

import imageLoader from "./image-loader";

const cache = new Map<string, string | null>();

/** "r g b" for a CSS `rgb(r g b / a)`, or null when the picture cannot be read. */
export function dominantColor(src: string): Promise<string | null> {
  if (cache.has(src)) return Promise.resolve(cache.get(src) ?? null);
  return new Promise((resolve) => {
    const image = new Image();
    image.crossOrigin = "anonymous";
    image.onload = () => {
      let found: string | null = null;
      try {
        const size = 24;
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = size;
        const context = canvas.getContext("2d", { willReadFrequently: true });
        context?.drawImage(image, 0, 0, size, size);
        const { data } = context!.getImageData(0, 0, size, size);
        let r = 0, g = 0, b = 0, weight = 0;
        for (let i = 0; i < data.length; i += 4) {
          const max = Math.max(data[i], data[i + 1], data[i + 2]);
          const min = Math.min(data[i], data[i + 1], data[i + 2]);
          // Saturated pixels count for more; near-black and near-white ones barely count.
          const w = 0.1 + (max - min) / 255 + (max > 30 && max < 235 ? 0.3 : 0);
          r += data[i] * w;
          g += data[i + 1] * w;
          b += data[i + 2] * w;
          weight += w;
        }
        if (weight > 0) found = `${Math.round(r / weight)} ${Math.round(g / weight)} ${Math.round(b / weight)}`;
      } catch {
        found = null;
      }
      cache.set(src, found);
      resolve(found);
    };
    image.onerror = () => {
      cache.set(src, null);
      resolve(null);
    };
    image.src = imageLoader({ src, width: 96 });
  });
}
