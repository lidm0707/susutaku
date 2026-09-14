/// Math graph rendering.
///
/// Polylines are GPU-rendered by the `wgpu-rs` wasm module (`Plotter`).
/// Build it with:
///   wasm-pack build crates/wgpu-rs --target web --out-dir web_ui/src/wasm/wgpu_rs
/// The UI falls back to a 2D-canvas renderer when the module is absent.
import { WASM_MODULE_URL } from "./capscreen.js";

export const SAMPLE_COUNT = 400;
const WASM_FALLBACK_URLS = [
  WASM_MODULE_URL,
  new URL("../wasm/wgpu_rs/wgpu_rs.js", import.meta.url).href,
];

export interface PlotSpec {
  exprs: string[];
  x: [number, number];
  y?: [number, number];
}

export interface Plotter {
  set_view(x_min: number, x_max: number, y_min: number, y_max: number): void;
  add_series(flat: number[]): void;
  clear_series(): void;
  render(): void;
}

type PlotterCtor = { new(target: HTMLCanvasElement): Promise<Plotter> };

export async function load_plotter_ctor(): Promise<PlotterCtor | null> {
  for (const url of WASM_FALLBACK_URLS) {
    try {
      const mod = await import(/* @vite-ignore */ url);
      const ctor = mod.Plotter ?? mod.default?.Plotter;
      if (ctor) return ctor as PlotterCtor;
    } catch {
      // try next candidate
    }
  }
  return null;
}

export async function make_plotter(target: HTMLCanvasElement): Promise<Plotter> {
  const Ctor = await load_plotter_ctor();
  if (Ctor) {
    try {
      return await new Ctor(target);
    } catch {
      // GPU adapter/device unavailable (e.g. no WebGPU on macOS Firefox)
    }
  }
  return make_fallback_plotter(target);
}

/// 2D-canvas stand-in with the same interface as the wasm Plotter.
export function make_fallback_plotter(target: HTMLCanvasElement): Plotter {
  const ctx = target.getContext("2d");
  if (!ctx) throw new Error("no 2d context");
  const g: CanvasRenderingContext2D = ctx;
  const BG = "#101014";
  const GRID = "#292933";
  const AXIS = "#6b6b73";
  const COLORS = ["#6badff", "#ff8c59", "#8ce68c", "#e68cf2", "#ffd959", "#73e6d9"];
  const view = { x0: -10, x1: 10, y0: -5, y1: 5 };
  const series: number[][] = [];

  function nice_step(range: number): number {
    if (!Number.isFinite(range) || range <= 0) return 1;
    const raw = range / 10;
    const mag = 10 ** Math.floor(Math.log10(Math.abs(raw)));
    const norm = raw / mag;
    const mult = norm < 1.5 ? 1 : norm < 3.5 ? 2 : norm < 7.5 ? 5 : 10;
    return mult * mag;
  }

  function render() {
    const w = target.width;
    const h = target.height;
    if (w === 0 || h === 0) return;
    g.fillStyle = BG;
    g.fillRect(0, 0, w, h);
    const gx = nice_step(view.x1 - view.x0);
    const gy = nice_step(view.y1 - view.y0);
    const px = (x: number) => ((x - view.x0) / (view.x1 - view.x0)) * w;
    const py = (y: number) => h - ((y - view.y0) / (view.y1 - view.y0)) * h;
    g.lineWidth = 1;
    for (let x = Math.ceil(view.x0 / gx) * gx; x <= view.x1; x += gx) {
      g.strokeStyle = Math.abs(x) <= gx / 2 ? AXIS : GRID;
      g.beginPath();
      g.moveTo(px(x), 0);
      g.lineTo(px(x), h);
      g.stroke();
    }
    for (let y = Math.ceil(view.y0 / gy) * gy; y <= view.y1; y += gy) {
      g.strokeStyle = Math.abs(y) <= gy / 2 ? AXIS : GRID;
      g.beginPath();
      g.moveTo(0, py(y));
      g.lineTo(w, py(y));
      g.stroke();
    }
    g.lineWidth = 2;
    series.forEach((pts, i) => {
      g.strokeStyle = COLORS[i % COLORS.length];
      g.beginPath();
      for (let k = 0; k + 1 < pts.length; k += 2) {
        const X = px(pts[k]);
        const Y = py(pts[k + 1]);
        if (k === 0) g.moveTo(X, Y);
        else g.lineTo(X, Y);
      }
      g.stroke();
    });
  }

  return {
    set_view(x0, x1, y0, y1) {
      view.x0 = x0;
      view.x1 = x1;
      view.y0 = y0;
      view.y1 = y1;
    },
    add_series(flat) {
      series.push(flat);
    },
    clear_series() {
      series.length = 0;
    },
    render,
  };
}

// --- expression evaluation ------------------------------------------------

type Tok = { t: "num"; v: number } | { t: "x" } | { t: "op"; v: string } | { t: "id"; v: string } | { t: "(" } | { t: ")" };

const FUNCS: Record<string, (x: number) => number> = {
  sin: Math.sin,
  cos: Math.cos,
  tan: Math.tan,
  asin: Math.asin,
  acos: Math.acos,
  atan: Math.atan,
  sqrt: Math.sqrt,
  abs: Math.abs,
  exp: Math.exp,
  ln: Math.log,
  log: Math.log10,
  floor: Math.floor,
  ceil: Math.ceil,
};
const CONSTS: Record<string, number> = { pi: Math.PI, e: Math.E };

function tokenize(src: string): Tok[] {
  const toks: Tok[] = [];
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (/\s/.test(c)) {
      i++;
    } else if (/[0-9.]/.test(c)) {
      let j = i;
      while (j < src.length && /[0-9.]/.test(src[j])) j++;
      toks.push({ t: "num", v: Number(src.slice(i, j)) });
      i = j;
    } else if (c === "x") {
      toks.push({ t: "x" });
      i++;
    } else if (/[a-z]/i.test(c)) {
      let j = i;
      while (j < src.length && /[a-z0-9_]/i.test(src[j])) j++;
      toks.push({ t: "id", v: src.slice(i, j).toLowerCase() });
      i = j;
    } else if ("+-*/^".includes(c)) {
      toks.push({ t: "op", v: c });
      i++;
    } else if (c === "(") {
      toks.push({ t: "(" });
      i++;
    } else if (c === ")") {
      toks.push({ t: ")" });
      i++;
    } else {
      throw new Error(`bad char "${c}"`);
    }
  }
  return toks;
}

function parse(tokens: Tok[]): (x: number) => number {
  let pos = 0;
  const peek = () => tokens[pos];
  const eat = () => tokens[pos++];

  function expr(): (x: number) => number {
    let left = term();
    while (peek()?.t === "op" && ((peek() as { v: string }).v === "+" || (peek() as { v: string }).v === "-")) {
      const op = (eat() as { v: string }).v;
      const right = term();
      const l = left;
      left = op === "+" ? (x) => l(x) + right(x) : (x) => l(x) - right(x);
    }
    return left;
  }

  function term(): (x: number) => number {
    let left = unary();
    while (peek()?.t === "op" && ((peek() as { v: string }).v === "*" || (peek() as { v: string }).v === "/")) {
      const op = (eat() as { v: string }).v;
      const right = unary();
      const l = left;
      left = op === "*" ? (x) => l(x) * right(x) : (x) => l(x) / right(x);
    }
    return left;
  }

  function unary(): (x: number) => number {
    if (peek()?.t === "op" && (peek() as { v: string }).v === "-") {
      eat();
      const inner = unary();
      return (x) => -inner(x);
    }
    return power();
  }

  function power(): (x: number) => number {
    const base = atom();
    if (peek()?.t === "op" && (peek() as { v: string }).v === "^") {
      eat();
      const exp = unary();
      return (x) => Math.pow(base(x), exp(x));
    }
    return base;
  }

  function atom(): (x: number) => number {
    const tk = eat();
    if (!tk) throw new Error("unexpected end");
    if (tk.t === "num") {
      const v = tk.v;
      return () => v;
    }
    if (tk.t === "x") return (x) => x;
    if (tk.t === "(") {
      const inner = expr();
      if (eat()?.t !== ")") throw new Error("missing )");
      return inner;
    }
    if (tk.t === "id") {
      const name = tk.v;
      if (peek()?.t === "(") {
        const fn = FUNCS[name];
        if (!fn) throw new Error(`unknown fn "${name}"`);
        eat();
        const arg = expr();
        if (eat()?.t !== ")") throw new Error("missing )");
        return (x) => fn(arg(x));
      }
      if (name in CONSTS) {
        const v = CONSTS[name];
        return () => v;
      }
      throw new Error(`unknown name "${name}"`);
    }
    throw new Error(`unexpected "${JSON.stringify(tk)}"`);
  }

  const fn = expr();
  if (pos !== tokens.length) throw new Error("trailing tokens");
  return fn;
}

/// Rewrite an equation "lhs = rhs" into an expression "rhs - lhs" (y isolated
/// on the lhs, substituted with 0); returns null when there is no "=" or y
/// appears outside the lhs.
function equation_to_expr(src: string): string | null {
  const eq = src.indexOf("=");
  if (eq === -1) return null;
  const lhs = src.slice(0, eq);
  const rhs = src.slice(eq + 1);
  if (/\by\b/i.test(rhs)) return null;
  if (!/\by\b/i.test(lhs)) return null;
  const lhs0 = lhs.replace(/\by\b/gi, "0");
  return `(${rhs}) - (${lhs0})`;
}

export function eval_expr(src: string, x: number): number {
  return parse(tokenize(equation_to_expr(src) ?? src))(x);
}

/// Sample an expression (or equation solvable for y) over [x0, x1] into flat
/// [x, y, …] pairs.
export function sample_expr(src: string, x0: number, x1: number): number[] {
  const fn = parse(tokenize(equation_to_expr(src) ?? src));
  const pts: number[] = [];
  const dx = (x1 - x0) / SAMPLE_COUNT;
  for (let i = 0; i <= SAMPLE_COUNT; i++) {
    const x = x0 + i * dx;
    pts.push(x, fn(x));
  }
  return pts;
}

/// Finite y extent over all series, padded — used for auto y range.
export function series_extent(series: number[][]): [number, number] | null {
  let lo = Infinity;
  let hi = -Infinity;
  for (const pts of series) {
    for (let k = 1; k < pts.length; k += 2) {
      const y = pts[k];
      if (Number.isFinite(y)) {
        lo = Math.min(lo, y);
        hi = Math.max(hi, y);
      }
    }
  }
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) return null;
  if (lo === hi) return [lo - 1, hi + 1];
  const pad = (hi - lo) * 0.1;
  return [lo - pad, hi + pad];
}

/// Parse a ```plot fence body: JSON {exprs:[…], x:[a,b], y?:[c,d]} or a bare
/// expression per line ("sin(x)", "x^2  # red" comments ignored).
export function parse_plot_spec(body: string): PlotSpec | null {
  const text = body.trim();
  if (text.startsWith("{")) {
    try {
      const raw = JSON.parse(text) as Partial<PlotSpec>;
      if (Array.isArray(raw.exprs) && raw.exprs.length > 0 && Array.isArray(raw.x) && raw.x.length === 2) {
        const exprs = raw.exprs.map(String);
        try {
          for (const e of exprs) parse(tokenize(equation_to_expr(e) ?? e));
        } catch {
          return null;
        }
        return {
          exprs: raw.exprs.map(String),
          x: [Number(raw.x[0]), Number(raw.x[1])],
          y: Array.isArray(raw.y) && raw.y.length === 2 ? [Number(raw.y[0]), Number(raw.y[1])] : undefined,
        };
      }
    } catch {
      return null;
    }
    return null;
  }
  const exprs = text
    .split("\n")
    .map((line) => line.replace(/#.*$/, "").trim())
    .filter((line) => line.length > 0);
  if (exprs.length === 0) return null;
  try {
    for (const e of exprs) parse(tokenize(equation_to_expr(e) ?? e));
  } catch {
    return null;
  }
  return { exprs, x: [-10, 10] };
}
