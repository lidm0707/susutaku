import { useEffect, useRef, useState } from "react";
import { Camera, Circle, ImagePlus, Loader2, Minus, Send, Trash2, Undo2 } from "lucide-react";
import { Modal } from "../ui/Overlay.js";
import {
  canvas_png_data_url,
  capture_screen,
  image_file_to_canvas,
  make_annotator,
  type ScreenAnnotator,
} from "../features/capscreen.js";

const CAPTURE_SCALE = 0.5;

type Tool = "line" | "circle";

interface Props {
  open: boolean;
  on_close: () => void;
  on_send: (image_data_url: string, note: string) => Promise<void>;
  /// Canvas prefilled by a drag-and-drop into the chat input; loaded once
  /// when the modal opens, then consumed.
  initial_image?: HTMLCanvasElement | null;
}

/// Annotate an image — captured screen, uploaded file, or dropped file — with
/// lines and circles (GPU via wgpu wasm when built, 2D-canvas fallback
/// otherwise), then hand the PNG to the caller.
export default function CapscreenModal({ open, on_close, on_send, initial_image }: Props) {
  const [shot, setShot] = useState<HTMLCanvasElement | null>(null);
  const [annotator, setAnnotator] = useState<ScreenAnnotator | null>(null);
  const [tool, setTool] = useState<Tool>("line");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [drag_over, setDragOver] = useState(false);
  const draw_ref = useRef<HTMLCanvasElement | null>(null);
  const drag_start = useRef<[number, number] | null>(null);
  const file_ref = useRef<HTMLInputElement | null>(null);

  async function load_canvas(source: HTMLCanvasElement) {
    setError("");
    try {
      setShot(source);
      const target = draw_ref.current;
      if (!target) return;
      target.width = source.width;
      target.height = source.height;
      const ann = await make_annotator(source, target);
      setAnnotator(ann);
      // the GPU annotator never paints until told — render the base image now
      ann.render();
    } catch (err) {
      setError(err instanceof Error ? err.message : "load failed");
    }
  }

  async function capture() {
    setError("");
    try {
      setAnnotator(null);
      const source = await capture_screen(CAPTURE_SCALE);
      await load_canvas(source);
    } catch (err) {
      setError(err instanceof Error ? err.message : "capture failed");
    }
  }

  async function load_file(file: File | undefined | null) {
    if (!file) return;
    setError("");
    try {
      setAnnotator(null);
      await load_canvas(await image_file_to_canvas(file));
    } catch (err) {
      setError(err instanceof Error ? err.message : "could not load image");
    }
  }

  // consume a canvas dropped into the chat input
  useEffect(() => {
    if (open && initial_image) {
      load_canvas(initial_image);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  function pos(e: React.PointerEvent<HTMLCanvasElement>): [number, number] {
    const rect = e.currentTarget.getBoundingClientRect();
    const sx = e.currentTarget.width / rect.width;
    const sy = e.currentTarget.height / rect.height;
    return [(e.clientX - rect.left) * sx, (e.clientY - rect.top) * sy];
  }

  function on_pointer_down(e: React.PointerEvent<HTMLCanvasElement>) {
    if (!annotator) return;
    drag_start.current = pos(e);
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  function on_pointer_move(e: React.PointerEvent<HTMLCanvasElement>) {
    const start = drag_start.current;
    if (!annotator || !start) return;
    const [x, y] = pos(e);
    annotator.set_preview(start[0], start[1], x, y, tool === "circle");
    annotator.render();
  }

  function end_stroke(e: React.PointerEvent<HTMLCanvasElement>, commit: boolean) {
    const start = drag_start.current;
    drag_start.current = null;
    if (!annotator || !start) return;
    annotator.clear_preview();
    if (commit) {
      const [x, y] = pos(e);
      if (tool === "line") {
        annotator.add_line(start[0], start[1], x, y);
      } else {
        // press = center, release point sets the radius
        const radius = Math.hypot(x - start[0], y - start[1]);
        if (radius > 1) annotator.add_circle(start[0], start[1], radius);
      }
    }
    annotator.render();
  }

  function undo() {
    annotator?.undo_line();
    annotator?.render();
  }

  function clear() {
    annotator?.clear_lines();
    annotator?.render();
  }

  async function send() {
    if (!draw_ref.current) return;
    setBusy(true);
    setError("");
    try {
      // webgpu presents on the next composited frame — read too early and the
      // export comes out blank/transparent (a black rectangle in the chat)
      annotator?.render();
      await new Promise<void>((r) =>
        requestAnimationFrame(() => requestAnimationFrame(() => r()))
      );
      const image = await canvas_png_data_url(draw_ref.current);
      await on_send(image, note.trim());
      annotator?.clear_lines();
      setAnnotator(null);
      setShot(null);
      setNote("");
      on_close();
    } catch (err) {
      setError(err instanceof Error ? err.message : "send failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal open={open} title="annotate image" on_close={on_close} wide>
      <div
        className="capscreen"
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragOver(false);
          load_file(e.dataTransfer.files?.[0]);
        }}
      >
        <div className="capscreen-actions">
          <button type="button" className="btn" onClick={capture} disabled={busy}>
            <Camera size={13} /> {shot ? "recapture screen" : "capture screen"}
          </button>
          <button type="button" className="btn" onClick={() => file_ref.current?.click()} disabled={busy}>
            <ImagePlus size={13} /> upload image
          </button>
          <input
            ref={file_ref}
            type="file"
            accept="image/*"
            hidden
            onChange={(e) => {
              load_file(e.target.files?.[0]);
              e.target.value = "";
            }}
          />
          {annotator && (
            <>
              <button
                type="button"
                className={tool === "line" ? "btn primary" : "btn"}
                onClick={() => setTool("line")}
                title="draw lines"
                aria-pressed={tool === "line"}
              >
                <Minus size={13} /> line
              </button>
              <button
                type="button"
                className={tool === "circle" ? "btn primary" : "btn"}
                onClick={() => setTool("circle")}
                title="draw circles (press = center, drag = radius)"
                aria-pressed={tool === "circle"}
              >
                <Circle size={13} /> circle
              </button>
              <button type="button" className="btn" onClick={undo} disabled={busy}>
                <Undo2 size={13} /> undo
              </button>
              <button type="button" className="btn" onClick={clear} disabled={busy}>
                <Trash2 size={13} /> clear
              </button>
              <button type="button" className="btn primary" onClick={send} disabled={busy}>
                {busy ? <Loader2 size={13} className="spin" /> : <Send size={13} />} send
              </button>
            </>
          )}
        </div>
        {error && <p className="error">{error}</p>}
        <canvas
          ref={draw_ref}
          className={`capscreen-canvas${drag_over ? " drop" : ""}`}
          onPointerDown={on_pointer_down}
          onPointerMove={on_pointer_move}
          onPointerUp={(e) => end_stroke(e, true)}
          onPointerCancel={(e) => end_stroke(e, false)}
          style={{ display: shot ? "block" : "none", maxWidth: "100%", touchAction: "none" }}
          aria-label="image annotation canvas"
        />
        {!shot && (
          <p className="empty">
            {drag_over
              ? "drop the image here"
              : "capture the screen, upload or drag an image — then drag to draw lines or circles on it"}
          </p>
        )}
      </div>
    </Modal>
  );
}
