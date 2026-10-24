import { useEffect, useRef, type ButtonHTMLAttributes, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";

type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  shortcut?: string;
  active?: boolean;
  size?: "sm" | "md";
};

/** Square icon button with an accessible label and native tooltip. */
export function IconButton({
  label,
  shortcut,
  active,
  size = "sm",
  className = "",
  children,
  ...rest
}: IconButtonProps) {
  const dim = size === "md" ? "h-8 w-8" : "h-6 w-6";
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={active}
      title={shortcut ? `${label} (${shortcut})` : label}
      className={`${dim} inline-flex shrink-0 items-center justify-center rounded-[5px] transition-colors disabled:pointer-events-none disabled:opacity-35 ${
        active ? "bg-accent-soft text-accent" : "text-dim hover:bg-hover hover:text-fg"
      } ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}

export function Section({
  title,
  open,
  onToggle,
  actions,
  children,
  grow,
}: {
  title: string;
  open: boolean;
  onToggle: () => void;
  actions?: ReactNode;
  children: ReactNode;
  grow?: boolean;
}) {
  return (
    <section
      className={`flex min-h-0 flex-col border-b border-line ${grow && open ? "flex-1" : ""}`}
    >
      <header className="flex h-8 shrink-0 items-center gap-1 pr-1.5 pl-2">
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={open}
          className="flex flex-1 items-center gap-1 text-[11px] font-semibold tracking-wide text-dim uppercase hover:text-fg"
        >
          <ChevronDown size={12} className={`transition-transform ${open ? "" : "-rotate-90"}`} />
          {title}
        </button>
        {open && actions}
      </header>
      {open && <div className="min-h-0 flex-1">{children}</div>}
    </section>
  );
}

export function Divider() {
  return <div className="mx-1 h-4 w-px shrink-0 bg-line" />;
}

/** Modal dialog with focus trap-lite: focuses first field, Esc closes. */
export function Dialog({
  title,
  onClose,
  children,
  width = 340,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  width?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    const first = el?.querySelector<HTMLElement>("input, button:not([data-dismiss])");
    first?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);
  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[14vh]"
      onPointerDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        style={{ width }}
        className="rounded-lg border border-line-strong bg-raised shadow-2xl shadow-black/40"
      >
        <div className="border-b border-line px-4 py-2.5 text-[13px] font-semibold">{title}</div>
        <div className="p-4">{children}</div>
      </div>
    </div>
  );
}

export function Button({
  variant = "secondary",
  className = "",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "secondary" }) {
  const style =
    variant === "primary"
      ? "bg-accent text-white hover:brightness-110"
      : "border border-line-strong bg-panel text-fg hover:bg-hover";
  return (
    <button
      type="button"
      className={`h-7 rounded-[5px] px-3 text-[12px] font-medium transition ${style} ${className}`}
      {...rest}
    />
  );
}

export const inputClass =
  "h-7 w-full rounded-[5px] border border-line-strong bg-panel px-2 text-[12px] text-fg outline-none focus:border-accent";
