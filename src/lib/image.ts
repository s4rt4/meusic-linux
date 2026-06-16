/**
 * Downscale a cover-art data URI to at most `max` px on its longest side and
 * re-encode as JPEG. Embedded covers are often 1500px+ multi-megabyte images;
 * the UI never shows them larger than ~360px, so capping the source keeps both
 * the cached string and the decoded bitmap small (big memory win during long
 * listening sessions). On any failure it returns the original src unchanged.
 */
export async function downscaleDataUri(src: string, max = 512): Promise<string> {
  try {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const im = new Image();
      im.onload = () => resolve(im);
      im.onerror = reject;
      im.src = src;
    });

    const scale = Math.min(1, max / Math.max(img.naturalWidth, img.naturalHeight));
    const w = Math.max(1, Math.round(img.naturalWidth * scale));
    const h = Math.max(1, Math.round(img.naturalHeight * scale));

    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) return src;
    ctx.drawImage(img, 0, 0, w, h);
    return canvas.toDataURL("image/jpeg", 0.85);
  } catch {
    return src;
  }
}
