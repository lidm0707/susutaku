import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from "react";

type ButtonVariant = "primary" | "ghost" | "danger";

const BUTTON_VARIANTS: ButtonVariant[] = ["primary", "ghost", "danger"];

export function Button({
  variant = "ghost",
  children,
  className,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant }) {
  const v = BUTTON_VARIANTS.includes(variant) ? variant : "ghost";
  return (
    <button className={className ? `btn btn-${v} ${className}` : `btn btn-${v}`} {...rest}>
      {children}
    </button>
  );
}

export function IconButton({
  title,
  children,
  className,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      title={title}
      aria-label={title}
      className={className ? `icon-btn ${className}` : "icon-btn"}
      {...rest}
    >
      {children}
    </button>
  );
}

export function Field({
  label,
  icon,
  children,
}: {
  label: string;
  icon?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="field">
      <label>
        {icon}
        {label}
      </label>
      {children}
    </div>
  );
}

export function TextInput({ className, ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={className ? `text-input ${className}` : "text-input"} {...rest} />;
}

export function TextArea({ className, ...rest }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea className={className ? `text-area ${className}` : "text-area"} {...rest} />;
}

export function Select({ className, children, ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select className={className ? `select ${className}` : "select"} {...rest}>
      {children}
    </select>
  );
}
