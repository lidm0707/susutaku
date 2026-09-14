import { useEffect, useRef } from "react";
import { make_plotter, sample_expr, series_extent, type PlotSpec, type Plotter } from "../features/graph.js";

const FALLBACK_W = 640;
const ASPECT = 0.625; // height / width

/// Canvas that renders a [`PlotSpec`] via the wgpu wasm Plotter (2D-canvas
/// fallback). Y range is auto-fit when `spec.y` is omitted.
export default function GraphView({ spec }: { spec: PlotSpec }) {
  const canvas_ref = useRef<HTMLCanvasElement | null>(null);
  const plotter_ref = useRef<Plotter | null>(null);
  const spec_ref = useRef<PlotSpec>(spec);
  spec_ref.current = spec;

  useEffect(() => {
    const canvas = canvas_ref.current;
    if (!canvas) return;
    let cancelled = false;
    make_plotter(canvas)
      .then((p) => {
        if (cancelled) return;
        plotter_ref.current = p;
        draw(p);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      plotter_ref.current = null;
    };
  }, []);

  useEffect(() => {
    if (plotter_ref.current) draw(plotter_ref.current);
  }, [spec]);

  function draw(p: Plotter) {
    const canvas = canvas_ref.current;
    const s = spec_ref.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const w = Math.max(1, Math.floor(rect.width) || FALLBACK_W);
    canvas.width = w;
    canvas.height = Math.max(1, Math.floor(w * ASPECT));
    const [x0, x1] = s.x;
    let series: number[][] = [];
    try {
      series = s.exprs.map((e) => sample_expr(e, x0, x1));
    } catch {
      return;
    }
    const [y0, y1] = s.y ?? series_extent(series) ?? [-1, 1];
    p.clear_series();
    p.set_view(x0, x1, y0, y1);
    series.forEach((pts) => p.add_series(pts));
    p.render();
  }

  return <canvas ref={canvas_ref} className="graph-canvas" aria-label="function plot" />;
}
