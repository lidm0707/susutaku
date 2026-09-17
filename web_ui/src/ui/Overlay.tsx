import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import type { ReactNode } from "react";

const ESC_KEY = "Escape";
const OVERLAY_ANIM_MS = 220;

// keep the overlay mounted briefly after `open` flips false so the
// exit animation has an element to play on
function use_closing(open: boolean) {
  const [mounted, set_mounted] = useState(open);
  useEffect(() => {
    if (open) {
      set_mounted(true);
      return;
    }
    if (!mounted) return;
    const t = setTimeout(() => set_mounted(false), OVERLAY_ANIM_MS);
    return () => clearTimeout(t);
  }, [open, mounted]);
  return mounted;
}

function use_overlay(open: boolean, on_close: () => void) {
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const on_key = (e: KeyboardEvent) => {
      if (e.key === ESC_KEY) on_close();
    };
    document.addEventListener("keydown", on_key);
    document.body.style.overflow = "hidden";
    return () => {
      document.removeEventListener("keydown", on_key);
      document.body.style.overflow = "";
    };
  }, [open, on_close]);
  useEffect(() => {
    if (open) box.current?.focus();
  }, [open]);
  return box;
}

// transform/filter on an ancestor (e.g. .dock-nav) becomes the containing
// block for position:fixed — portal to body so overlays always span the viewport
function Portal({ children }: { children: ReactNode }) {
  return createPortal(children, document.body);
}

type HeaderProps = { title: ReactNode; on_close: () => void };

function OverlayHeader({ title, on_close }: HeaderProps) {
  return (
    <header>
      <h2>{title}</h2>
      <button type="button" onClick={on_close} title="close" aria-label="close">
        <X size={14} />
      </button>
    </header>
  );
}

type ModalProps = {
  open: boolean;
  title: ReactNode;
  on_close: () => void;
  children: ReactNode;
  wide?: boolean;
  className?: string;
  docked?: boolean;
  docked_left?: boolean;
};

export function Modal({ open, title, on_close, children, wide, className, docked, docked_left }: ModalProps) {
  const box = use_overlay(open, on_close);
  const mounted = use_closing(open);
  if (!mounted) return null;
  return (
    <Portal>
    <div
      className={[
        "overlay",
        open ? "" : "closing",
        docked ? "docked" : docked_left ? "docked docked-left" : "",
      ].filter(Boolean).join(" ")}
      onMouseDown={(e) => e.target === e.currentTarget && on_close()}
    >
      <div
        ref={box}
        className={[
          "overlay-box",
          "modal",
          wide ? "wide" : "",
          docked ? "docked-right" : "",
          docked_left ? "docked-left" : "",
          open ? "" : "closing",
          className ?? "",
        ].filter(Boolean).join(" ")}
        role="dialog"
        aria-modal="true"
        tabIndex={-1}
      >
        <OverlayHeader title={title} on_close={on_close} />
        {children}
      </div>
    </div>
    </Portal>
  );
}

type SlideOverProps = {
  open: boolean;
  title: ReactNode;
  on_close: () => void;
  children: ReactNode;
};

export function SlideOver({ open, title, on_close, children }: SlideOverProps) {
  const box = use_overlay(open, on_close);
  const mounted = use_closing(open);
  if (!mounted) return null;
  return (
    <Portal>
    <div className={`overlay${open ? "" : " closing"}`} onMouseDown={(e) => e.target === e.currentTarget && on_close()}>
      <div
        ref={box}
        className={`overlay-box slideover${open ? "" : " closing"}`}
        role="dialog"
        aria-modal="true"
        tabIndex={-1}
      >
        <OverlayHeader title={title} on_close={on_close} />
        <div className="slideover-body">{children}</div>
      </div>
    </div>
    </Portal>
  );
}

type PromptModalProps = {
  open: boolean;
  title: string;
  placeholder: string;
  on_close: () => void;
  on_submit: (value: string) => void;
};

export function PromptModal({ open, title, placeholder, on_close, on_submit }: PromptModalProps) {
  const value = useRef("");
  return (
    <Modal open={open} title={title} on_close={on_close}>
      <form
        className="modal-form"
        onSubmit={(e) => {
          e.preventDefault();
          const v = value.current.trim();
          if (!v) return;
          on_submit(v);
        }}
      >
        <input
          autoFocus
          placeholder={placeholder}
          onChange={(e) => (value.current = e.target.value)}
        />
        <button type="submit">create</button>
      </form>
    </Modal>
  );
}
