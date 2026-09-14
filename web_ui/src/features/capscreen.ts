/// Screen capture + annotation support.
///
/// Line drawing is GPU-rendered by the `wgpu-rs` wasm module
/// (`ScreenAnnotator`). Build it with:
///   wasm-pack build crates/wgpu-rs --target web --out-dir web_ui/src/wasm/wgpu_rs
/// The UI falls back to a 2D-canvas renderer when the module is absent.

export const WASM_MODULE_URL = new URL("../wasm/wgpu_rs/wgpu_rs.js", import.meta.url).href;
export const WASM_BINARY_URL = new URL("../wasm/wgpu_rs/wgpu_rs_bg.wasm", import.meta.url).href;

/// Import the wasm-bindgen glue and run its default `init()` with an explicit
/// binary URL (the glue's own relative fetch breaks once Vite renames chunks).
/// Returns null when either part fails to load.
export async function load_wasm_module(): Promise<Record<string, unknown> | null> {
  try {
    const mod = await import(/* @vite-ignore */ WASM_MODULE_URL);
    const init = (mod.default ?? mod.init) as
      | ((arg: { module_or_path: string }) => Promise<unknown>)
      | undefined;
    if (typeof init !== "function") return null;
    await init({ module_or_path: WASM_BINARY_URL });
    return mod as Record<string, unknown>;
  } catch {
    return null;
  }
}

export interface ScreenAnnotator {
  add_line(x1: number, y1: number, x2: number, y2: number): void;
  add_circle(cx: number, cy: number, radius: number): void;
  /// Live in-progress stroke while dragging; `circle` = start is the center.
  set_preview(x1: number, y1: number, x2: number, y2: number, circle: boolean): void;
  clear_preview(): void;
  undo_line(): void;
  clear_lines(): void;
  line_count(): number;
  render(): void;
  to_png(): Promise<Blob>;
}

export interface AnnotatorCtor {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  new(): any;
  new(source: HTMLCanvasElement, target: HTMLCanvasElement): Promise<ScreenAnnotator>;
}

export async function load_annotator_ctor(): Promise<AnnotatorCtor | null> {
  const mod = await load_wasm_module();
  if (!mod) return null;
  return (mod.ScreenAnnotator ?? (mod as { default?: { ScreenAnnotator?: AnnotatorCtor } }).default?.ScreenAnnotator) as AnnotatorCtor | undefined ?? null;
}

export function make_fallback_annotator(
  source: HTMLCanvasElement,
  target: HTMLCanvasElement
): ScreenAnnotator {
  target.width = source.width;
  target.height = source.height;
  const ctx = target.getContext("2d");
  if (!ctx) throw new Error("no 2d context");
  const g: CanvasRenderingContext2D = ctx;
  g.drawImage(source, 0, 0);
  g.lineWidth = 3;
  g.strokeStyle = "#f33";
  g.lineCap = "round";

  /// Committed strokes with their draw params, replayed after each preview
  /// frame (2D canvas has no GPU history to blend over).
  type Stroke = { kind: "line"; pts: [number, number, number, number] } | { kind: "circle"; cx: number; cy: number; r: number };
  const strokes: Stroke[] = [];
  let snapshot: ImageData | null = null;
  let preview: (Stroke & { done: boolean }) | null = null;

  function stroke_path(s: Stroke) {
    g.beginPath();
    if (s.kind === "line") {
      g.moveTo(s.pts[0], s.pts[1]);
      g.lineTo(s.pts[2], s.pts[3]);
    } else {
      g.arc(s.cx, s.cy, s.r, 0, Math.PI * 2);
    }
    g.stroke();
  }

  function repaint() {
    if (snapshot) g.putImageData(snapshot, 0, 0);
    else g.drawImage(source, 0, 0);
    for (const s of strokes) stroke_path(s);
    if (preview) stroke_path(preview);
  }

  return {
    add_line(x1, y1, x2, y2) {
      strokes.push({ kind: "line", pts: [x1, y1, x2, y2] });
      preview = null;
      repaint();
    },
    add_circle(cx, cy, radius) {
      strokes.push({ kind: "circle", cx, cy, r: radius });
      preview = null;
      repaint();
    },
    set_preview(x1, y1, x2, y2, circle) {
      if (!snapshot) snapshot = g.getImageData(0, 0, target.width, target.height);
      preview = circle
        ? { kind: "circle", cx: x1, cy: y1, r: Math.hypot(x2 - x1, y2 - y1), done: false }
        : { kind: "line", pts: [x1, y1, x2, y2], done: false };
      repaint();
    },
    clear_preview() {
      if (preview) {
        preview = null;
        repaint();
      }
      snapshot = null;
    },
    undo_line() {
      strokes.pop();
      repaint();
    },
    clear_lines() {
      strokes.length = 0;
      repaint();
    },
    line_count() {
      return strokes.length;
    },
    render() {},
    to_png: () =>
      new Promise((resolve, reject) =>
        target.toBlob((b) => (b ? resolve(b) : reject(new Error("to_blob failed"))), "image/png")
      ),
  };
}

export async function make_annotator(
  source: HTMLCanvasElement,
  target: HTMLCanvasElement
): Promise<ScreenAnnotator> {
  const Ctor = await load_annotator_ctor();
  if (Ctor) {
    try {
      return await (Ctor as unknown as { new: (s: HTMLCanvasElement, t: HTMLCanvasElement) => Promise<ScreenAnnotator> }).new(source, target);
    } catch {
      // GPU adapter/device unavailable
    }
  }
  return make_fallback_annotator(source, target);
}

/// Capture one frame of the screen into a canvas (getDisplayMedia).
export async function capture_screen(scale_down = 1): Promise<HTMLCanvasElement> {
  const stream = await navigator.mediaDevices.getDisplayMedia({ video: true });
  try {
    const video = document.createElement("video");
    video.srcObject = stream;
    video.muted = true;
    await new Promise<void>((resolve) => {
      video.onloadedmetadata = () => resolve();
    });
    await video.play();
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.floor(video.videoWidth / scale_down));
    canvas.height = Math.max(1, Math.floor(video.videoHeight / scale_down));
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("no 2d context");
    ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
    video.pause();
    return canvas;
  } finally {
    stream.getTracks().forEach((t) => t.stop());
  }
}

export async function canvas_png_data_url(canvas: HTMLCanvasElement): Promise<string> {
  return new Promise((resolve, reject) =>
    canvas.toBlob(
      (blob) => {
        if (!blob) return reject(new Error("to_blob failed"));
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as string);
        reader.onerror = () => reject(new Error("read failed"));
        reader.readAsDataURL(blob);
      },
      "image/png"
    )
  );
}

const MAX_SOURCE_EDGE_PX = 1600;

/// Draw any image source (uploaded file, dropped file) into a fresh canvas,
/// scaled down to a sane annotation/upload size.
export async function image_file_to_canvas(file: File): Promise<HTMLCanvasElement> {
  if (!file.type.startsWith("image/")) throw new Error("not an image");
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, MAX_SOURCE_EDGE_PX / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.floor(bitmap.width * scale));
  canvas.height = Math.max(1, Math.floor(bitmap.height * scale));
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no 2d context");
  ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
  return canvas;
}
