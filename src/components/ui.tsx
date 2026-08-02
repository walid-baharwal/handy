import type { ReactNode } from "react";

export function NavButton({
  active,
  onClick,
  icon,
  label,
  badge,
}: {
  active: boolean;
  onClick: () => void;
  icon: string;
  label: string;
  badge?: number;
}) {
  return (
    <button className={active ? "nav-active" : ""} onClick={onClick}>
      <span>{icon}</span>
      {label}
      {badge && <b>{badge}</b>}
    </button>
  );
}

export function PageHeader({
  eyebrow,
  title,
  description,
  action,
}: {
  eyebrow: string;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <header className="page-header">
      <div>
        <small>{eyebrow}</small>
        <h1>{title}</h1>
        <p>{description}</p>
      </div>
      {action}
    </header>
  );
}

export function Modal({
  title,
  subtitle,
  onClose,
  wide,
  children,
}: {
  title: string;
  subtitle: string;
  onClose: () => void;
  wide?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(event) => event.target === event.currentTarget && onClose()}
    >
      <div className={`modal ${wide ? "modal-wide" : ""}`} role="dialog" aria-modal="true">
        <header>
          <div>
            <h2>{title}</h2>
            <p>{subtitle}</p>
          </div>
          <button aria-label="Close dialog" onClick={onClose}>
            ×
          </button>
        </header>
        {children}
      </div>
    </div>
  );
}

export function Empty({
  title,
  text,
  action,
}: {
  title: string;
  text: string;
  action: () => void;
}) {
  return (
    <div className="empty-state">
      <span>
        <Glyph name="plus" />
      </span>
      <h2>{title}</h2>
      <p>{text}</p>
      <button className="primary" onClick={action}>
        Get started <Glyph name="launch" />
      </button>
    </div>
  );
}

export function Status({ label }: { label: string }) {
  return (
    <span className={`status-pill ${label.toLowerCase().replace(" ", "-")}`}>
      <i />
      {label}
    </span>
  );
}

export function Glyph({ name }: { name: "folder" | "launch" | "plus" | "recipe" | "stop" }) {
  const paths = {
    folder: <path d="M3 6.5h6l1.7 2H21v10.8H3z" />,
    launch: (
      <>
        <path d="M5 19 19 5" />
        <path d="M9 5h10v10" />
      </>
    ),
    plus: (
      <>
        <path d="M12 4v16" />
        <path d="M4 12h16" />
      </>
    ),
    recipe: (
      <>
        <path d="M5 6h14" />
        <path d="M5 12h14" />
        <path d="M5 18h14" />
        <circle cx="8" cy="6" r="1" fill="currentColor" />
        <circle cx="16" cy="12" r="1" fill="currentColor" />
        <circle cx="11" cy="18" r="1" fill="currentColor" />
      </>
    ),
    stop: <rect x="6" y="6" width="12" height="12" fill="currentColor" stroke="none" />,
  };

  return (
    <svg
      className="glyph"
      viewBox="0 0 24 24"
      aria-hidden="true"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="square"
      strokeLinejoin="miter"
    >
      {paths[name]}
    </svg>
  );
}
