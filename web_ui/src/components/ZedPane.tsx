import { Sparkles } from "lucide-react";

/// Zed-like read-only editor pane: tab strip with a file label, line-number
/// gutter, monospace body. Used for agent text in the card detail and chat.
export default function ZedPane({
  title = "agent.txt",
  text,
  empty_hint,
}: {
  title?: string;
  text: string;
  empty_hint: string;
}) {
  const lines = text ? text.split("\n") : [];
  return (
    <section className="zed-pane" aria-label="agent text">
      <header className="zed-tabs">
        <span className="zed-tab active">
          <Sparkles size={11} /> {title}
        </span>
      </header>
      <div className="zed-body">
        {lines.length === 0 ? (
          <p className="empty">{empty_hint}</p>
        ) : (
          <ol className="zed-lines">
            {lines.map((l, i) => (
              <li key={i}>
                <span className="zed-ln">{i + 1}</span>
                <span className="zed-line">{l || " "}</span>
              </li>
            ))}
          </ol>
        )}
      </div>
    </section>
  );
}
