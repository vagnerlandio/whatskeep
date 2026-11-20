// Building blocks of the import screens: a header with "back", a panel with
// sections, selectable cards, segmented controls, switches and short notes.
// Styles in App.css ("flow" section).
import { ReactNode } from "react";
import { Icon, IconName } from "@wk/viewer";

/** Page title with an optional "back" button and actions on the right. */
export function FlowHeader({
  title,
  subtitle,
  onBack,
  backLabel,
  actions,
}: {
  title: string;
  subtitle?: ReactNode;
  onBack?: () => void;
  backLabel?: string;
  actions?: ReactNode;
}) {
  return (
    <header className="flow-head">
      {onBack && (
        <button className="icon-round" onClick={onBack} title={backLabel} aria-label={backLabel}>
          <Icon name="back" size={20} />
        </button>
      )}
      <div className="flow-head-text">
        <h1>{title}</h1>
        {subtitle && <p className="muted">{subtitle}</p>}
      </div>
      {actions && <div className="flow-head-actions">{actions}</div>}
    </header>
  );
}

/** White panel holding the sections of a step. */
export function Panel({ children, className }: { children: ReactNode; className?: string }) {
  return <section className={className ? `panel ${className}` : "panel"}>{children}</section>;
}

/** A block of the panel, with a green label (like WhatsApp's settings). */
export function Section({ label, aside, children }: { label?: string; aside?: ReactNode; children: ReactNode }) {
  return (
    <div className="panel-section">
      {(label || aside) && (
        <div className="section-label">
          {label && <span>{label}</span>}
          {aside}
        </div>
      )}
      {children}
    </div>
  );
}

/** Buttons at the bottom of a panel; `info` on the left. */
export function PanelActions({ info, children }: { info?: ReactNode; children: ReactNode }) {
  return (
    <div className="panel-actions">
      {info && <span className="muted small">{info}</span>}
      <div className="panel-actions-buttons">{children}</div>
    </div>
  );
}

/** Selectable card (one of a group). */
export function OptionCard({
  selected,
  onSelect,
  icon,
  title,
  hint,
  extra,
  disabled,
  name,
}: {
  selected: boolean;
  onSelect: () => void;
  icon?: IconName;
  title: ReactNode;
  hint?: ReactNode;
  extra?: ReactNode;
  disabled?: boolean;
  name: string;
}) {
  return (
    <label className={`option-card${selected ? " selected" : ""}${disabled ? " disabled" : ""}`}>
      <input type="radio" name={name} checked={selected} disabled={disabled} onChange={onSelect} />
      {icon && (
        <span className="option-icon">
          <Icon name={icon} size={20} />
        </span>
      )}
      <span className="option-body">
        <span className="option-title">{title}</span>
        {hint && <span className="muted small">{hint}</span>}
        {extra}
      </span>
      <span className="option-check" aria-hidden>
        <Icon name="check" size={14} strokeWidth={3} />
      </span>
    </label>
  );
}

/** Two or three mutually exclusive choices side by side. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="segmented" role="radiogroup">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          className={value === o.value ? "active" : ""}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** On/off row: label (and detail) on the left, switch on the right. */
export function SwitchRow({
  checked,
  onChange,
  icon,
  label,
  detail,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  icon?: IconName;
  label: string;
  detail?: ReactNode;
}) {
  return (
    <label className="switch-row">
      {icon && <Icon name={icon} size={20} className="switch-icon" />}
      <span className="switch-text">
        <span>{label}</span>
        {detail && <span className="muted small">{detail}</span>}
      </span>
      <input type="checkbox" className="switch" checked={checked} onChange={(e) => onChange(e.target.checked)} />
    </label>
  );
}

type Tone = "info" | "ok" | "warn" | "danger";
const TONE_ICON: Record<Tone, IconName> = { info: "info", ok: "checkCircle", warn: "warning", danger: "alert" };

/** A short note with an icon. */
export function Note({ tone = "info", icon, children }: { tone?: Tone; icon?: IconName; children: ReactNode }) {
  return (
    <div className={`note ${tone}`}>
      <Icon name={icon ?? TONE_ICON[tone]} size={18} />
      <div>{children}</div>
    </div>
  );
}

/** Extra explanation, closed by default. */
export function More({ summary, children }: { summary: string; children: ReactNode }) {
  return (
    <details className="disclosure">
      <summary>
        <Icon name="chevronRight" size={16} />
        {summary}
      </summary>
      <div className="disclosure-body">{children}</div>
    </details>
  );
}

/** Icon in a tinted circle (screen headers, results). */
export function Badge({ icon, tone = "ok", spin }: { icon: IconName; tone?: Tone; spin?: boolean }) {
  return (
    <span className={`round-badge ${tone}${spin ? " spinning" : ""}`}>
      <Icon name={icon} size={26} />
    </span>
  );
}
