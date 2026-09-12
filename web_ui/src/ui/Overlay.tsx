import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import type { ReactNode } from "react";

const ESC_KEY = "Escape";

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
  if (!open) return null;
  return (
    <Portal>
    <div
      className={docked || docked_left ? "overlay docked docked-left" : "overlay"}
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
  if (!open) return null;
  return (
    <Portal>
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && on_close()}>
      <div
        ref={box}
        className="overlay-box slideover"
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
