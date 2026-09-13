import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Download, Paperclip, Trash2, Upload } from "lucide-react";
import {
  AttachmentInfo,
  connect_events,
  delete_attachment,
  fetch_attachment_blob,
  fetch_attachments,
  upload_attachment,
} from "../lib.js";
import { toast } from "../ui/Toast.jsx";

function fmt_size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function fmt_date(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString();
}

export default function Attachments() {
  const navigate = useNavigate();
  const [items, setItems] = useState<AttachmentInfo[]>([]);
  const [busy, setBusy] = useState(false);

  async function load() {
    try {
      setItems(await fetch_attachments());
    } catch (err) {
      toast(String(err), "error");
    }
  }

  useEffect(() => {
    load();
    return connect_events((e) => {
      if (e.kind === "attachment") load();
    });
  }, []);

  async function on_upload(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    setBusy(true);
    try {
      await upload_attachment(file);
      toast("attachment uploaded");
      await load();
    } catch (err) {
      toast(String(err), "error");
    } finally {
      setBusy(false);
    }
  }

  async function on_view(item: AttachmentInfo) {
    try {
      const blob = await fetch_attachment_blob(item.path);
      window.open(URL.createObjectURL(blob), "_blank");
    } catch (err) {
      toast(String(err), "error");
    }
  }

  async function on_delete(item: AttachmentInfo) {
    if (!window.confirm(`delete ${item.name}?`)) return;
    try {
      await delete_attachment(item.path);
      toast("attachment deleted");
      await load();
    } catch (err) {
      toast(String(err), "error");
    }
  }

  return (
    <main className="chat kanban-page attachments-page">
      <header>
        <h1>attachments</h1>
        <span className="sub">{items.length} files</span>
        <label className="btn btn-primary">
          <Upload size={14} />
          {busy ? "uploading…" : "upload"}
          <input type="file" hidden onChange={on_upload} disabled={busy} />
        </label>
      </header>
      {items.length === 0 ? (
        <p className="empty">no attachments uploaded yet</p>
      ) : (
        <table className="attachments-table">
          <thead>
            <tr>
              <th scope="col">name</th>
              <th scope="col">size</th>
              <th scope="col">uploaded</th>
              <th scope="col">held by cards</th>
              <th scope="col" aria-label="actions" />
            </tr>
          </thead>
          <tbody>
            {items.map((item) => (
              <tr key={item.path}>
                <td>
                  <Paperclip size={12} aria-hidden /> {item.name}
                </td>
                <td>{fmt_size(item.size)}</td>
                <td>{fmt_date(item.modified)}</td>
                <td>
                  {item.cards.length === 0 ? (
                    <span className="empty">—</span>
                  ) : (
                    item.cards.map((c) => (
                      <button
                        key={c.id}
                        type="button"
                        className="chip"
                        title="open kanban board"
                        onClick={() => navigate("/kanban")}
                      >
                        {c.title}
                      </button>
                    ))
                  )}
                </td>
                <td>
                  <button type="button" onClick={() => on_view(item)} title="view" aria-label={`view ${item.name}`}>
                    <Download size={14} />
                  </button>
                  <button
                    type="button"
                    onClick={() => on_delete(item)}
                    title="delete"
                    aria-label={`delete ${item.name}`}
                  >
                    <Trash2 size={14} />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </main>
  );
}
