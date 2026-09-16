type LineKind = "add" | "del" | "hunk" | "meta" | "ctx";

interface DiffLine {
  kind: LineKind;
  text: string;
}

const DIFF_FILE_HEADER = "diff --git ";
const NO_CHANGES = "(no changes)";

const PREFIX_KIND: Record<string, LineKind> = {
  "+": "add",
  "-": "del",
  "@": "hunk",
};

function kind_of(text: string): LineKind {
  if (text.startsWith("@@")) return "hunk";
  if (text.startsWith(DIFF_FILE_HEADER) || text.startsWith("index ") || text.startsWith("---") || text.startsWith("+++"))
    return "meta";
  return PREFIX_KIND[text[0]] ?? "ctx";
}

function parse(diff: string): DiffLine[] {
  return diff.split("\n").map((text) => ({ kind: kind_of(text), text }));
}

export interface DiffFile {
  path: string;
  adds: number;
  dels: number;
  text: string;
}

// split a unified diff into per-file sections with add/del counts
export function parse_diff_files(diff: string): DiffFile[] {
  const files: DiffFile[] = [];
  let cur: DiffFile | null = null;
  for (const text of diff.split("\n")) {
    if (text.startsWith(DIFF_FILE_HEADER)) {
      const path = text.slice(DIFF_FILE_HEADER.length).split(" ").pop()?.replace(/^\w\//, "") ?? "?";
      cur = { path: path.replace(/^b\//, ""), adds: 0, dels: 0, text: "" };
      files.push(cur);
    }
    if (!cur) continue;
    cur.text += (cur.text ? "\n" : "") + text;
    if (text.startsWith("+") && !text.startsWith("+++")) cur.adds += 1;
    if (text.startsWith("-") && !text.startsWith("---")) cur.dels += 1;
  }
  return files;
}

export function DiffFileList({ diff, on_pick }: { diff: string; on_pick?: (f: DiffFile) => void }) {
  const files = parse_diff_files(diff);
  if (files.length === 0) return null;
  return (
    <ul className="diff-file-list" aria-label="changed files">
      {files.map((f) => (
        <li key={f.path}>
          <button type="button" className="diff-file-item" onClick={() => on_pick?.(f)}>
            <span className="diff-file-path">{f.path}</span>
            <span className="diff-file-stats">
              <span className="diff-stat-add">+{f.adds}</span>
              <span className="diff-stat-del">−{f.dels}</span>
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}

export function DiffStats({ diff }: { diff: string }) {
  const files = parse_diff_files(diff);
  const adds = files.reduce((n, f) => n + f.adds, 0);
  const dels = files.reduce((n, f) => n + f.dels, 0);
  return (
    <span className="diff-stats">
      {files.length} file{files.length === 1 ? "" : "s"}{" "}
      <span className="diff-stat-add">+{adds}</span>{" "}
      <span className="diff-stat-del">−{dels}</span>
    </span>
  );
}

export default function DiffView({ diff, rows = 16 }: { diff: string; rows?: number }) {
  if (!diff || diff === NO_CHANGES) {
    return <pre className="diff-view diff-empty">{diff || "…"}</pre>;
  }
  const lines = parse(diff);
  return (
    <div className="diff-view" role="figure" aria-label="diff" style={{ maxHeight: `${rows * 1.5}rem` }}>
      {lines.map((l, i) => (
        <div key={i} className={`diff-line diff-${l.kind}`}>
          <span className="diff-sign">{l.kind === "add" ? "+" : l.kind === "del" ? "-" : " "}</span>
          <span className="diff-text">{l.text.replace(/^[+\- ]/, "")}</span>
        </div>
      ))}
    </div>
  );
}
