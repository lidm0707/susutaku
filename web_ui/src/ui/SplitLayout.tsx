export type SideItem = {
  id: string;
  name: string;
  desc: string;
  icon: React.ReactNode;
};

export function SplitLayout({
  items,
  active,
  on_pick,
  footer,
  children,
  label,
}: {
  items: SideItem[];
  active: string;
  on_pick: (id: string) => void;
  footer?: React.ReactNode;
  children: React.ReactNode;
  label: string;
}) {
  return (
    <div className="split-layout">
      <nav className="split-side" aria-label={label}>
        <div className="split-side-items">
          {items.map((it) => (
            <button
              key={it.id}
              className={`split-item${active === it.id ? " active" : ""}`}
              aria-current={active === it.id}
              onClick={() => on_pick(it.id)}
            >
              <span className="split-item-icon">{it.icon}</span>
              <span className="split-item-text">
                <span className="split-item-name">{it.name}</span>
                <span className="split-item-desc">{it.desc}</span>
              </span>
            </button>
          ))}
        </div>
        {footer}
      </nav>
      <div className="split-detail">{children}</div>
    </div>
  );
}
