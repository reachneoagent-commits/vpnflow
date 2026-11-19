import { X } from "lucide-react";
import { type ButtonHTMLAttributes, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { HELP, type HelpKey, type HelpTopic } from "../lib/help";

type Variant = "primary" | "secondary" | "ghost" | "danger";

const variants: Record<Variant, string> = {
  primary: "bg-accent text-accent-fg hover:bg-(--btn-hover) border-transparent",
  secondary: "bg-chip text-fg border-transparent hover:brightness-95",
  ghost: "bg-transparent text-fg border-transparent hover:bg-chip",
  danger: "bg-danger text-white border-transparent hover:brightness-110",
};

export function Button({
  variant = "secondary",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      type="button"
      className={`inline-flex items-center justify-center gap-1.5 h-9 whitespace-nowrap shrink-0 rounded-[10px] border px-5 text-[13px] font-bold transition-[background-color,filter] disabled:cursor-not-allowed disabled:opacity-50 ${variants[variant]} ${className}`}
      {...props}
    />
  );
}

/** Small "?" button that opens a card: what it is, why, and an example. */
export function InfoTip({ topic }: { topic: HelpKey }) {
  const h: HelpTopic = HELP[topic];
  const id = useId();
  const btn = useRef<HTMLButtonElement>(null);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  const open = pos !== null;
  const show = () => {
    const r = btn.current?.getBoundingClientRect();
    if (!r) return;
    const w = 288;
    setPos({ top: r.bottom + 6, left: Math.max(8, Math.min(r.left - 8, window.innerWidth - w - 8)) });
  };
  const hide = () => setPos(null);
  return (
    <span className="relative inline-flex align-middle">
      <button
        ref={btn}
        type="button"
        aria-label={`What is ${h.title}?`}
        aria-expanded={open}
        aria-describedby={open ? id : undefined}
        onClick={() => (open ? hide() : show())}
        onMouseEnter={show}
        onMouseLeave={hide}
        onFocus={show}
        onBlur={hide}
        onKeyDown={(e) => e.key === "Escape" && hide()}
        className="ml-1.5 inline-flex h-4 w-4 items-center justify-center rounded-full border border-muted/60 text-[10px] leading-none font-semibold text-muted hover:border-fg hover:text-fg"
      >
        ?
      </button>
      {open && (
        <span
          id={id}
          role="tooltip"
          style={{ position: "fixed", top: pos.top, left: pos.left, width: 288 }}
          className="z-50 rounded-lg border border-line bg-surface p-3 text-left text-[12.5px] leading-snug font-normal text-fg shadow-lg"
        >
          <strong className="block text-[13px]">{h.title}</strong>
          <span className="mt-1 block">{h.what}</span>
          <span className="mt-1.5 block text-muted">
            <b className="font-semibold text-fg">Why: </b>
            {h.why}
          </span>
          <span className="mt-1.5 block text-muted">
            <b className="font-semibold text-fg">Example: </b>
            {h.example}
          </span>
          <span className="mt-1.5 block text-muted">
            <b className="font-semibold text-fg">Good to know: </b>
            {h.tradeoff}
          </span>
          {h.options && (
            <span className="mt-1.5 block text-muted">
              <b className="font-semibold text-fg">The choices: </b>
              {h.options.map((o) => (
                <span key={o.name} className="mt-0.5 block">
                  <b className="font-semibold text-fg">{o.name}: </b>
                  {o.text}
                </span>
              ))}
            </span>
          )}
          {h.pick && (
            <span className="mt-1.5 block text-muted">
              <b className="font-semibold text-fg">Which to pick: </b>
              {h.pick}
            </span>
          )}
        </span>
      )}
    </span>
  );
}

export function Switch({
  checked,
  onChange,
  label,
  description,
  disabled,
  help,
}: {
  help?: HelpKey;
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  description?: ReactNode;
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <div className="flex items-start justify-between gap-6 py-3">
      <div className="min-w-0">
        <label htmlFor={id} className="inline-block font-medium">
          {label}
        </label>
        {help && <InfoTip topic={help} />}
        {description && (
          <div id={`${id}-d`} className="mt-0.5 max-w-[60ch] text-[13px] text-muted">
            {description}
          </div>
        )}
      </div>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        aria-describedby={description ? `${id}-d` : undefined}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={`relative mt-0.5 h-6 w-10 shrink-0 rounded-full transition-colors disabled:opacity-50 ${
          checked ? "bg-accent" : "bg-chip"
        }`}
      >
        <span
          className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-[left] ${
            checked ? "left-[18px]" : "left-0.5"
          }`}
        />
      </button>
    </div>
  );
}

export function TextField({
  label,
  value,
  onChange,
  error,
  hint,
  mono,
  placeholder,
  type = "text",
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  error?: string;
  hint?: string;
  mono?: boolean;
  placeholder?: string;
  type?: string;
}) {
  const id = useId();
  return (
    <div>
      <label htmlFor={id} className="mb-1 block text-[13px] font-medium">
        {label}
      </label>
      <input
        id={id}
        type={type}
        value={value}
        placeholder={placeholder}
        spellCheck={false}
        aria-invalid={!!error}
        aria-describedby={error || hint ? `${id}-m` : undefined}
        onChange={(e) => onChange(e.target.value)}
        className={`w-full text-fg placeholder:text-faint ${
          mono ? "font-mono" : ""
        } ${error ? "border-danger" : "border-line"}`}
      />
      {(error || hint) && (
        <p id={`${id}-m`} className={`mt-1 text-[12px] ${error ? "text-danger" : "text-muted"}`}>
          {error ?? hint}
        </p>
      )}
    </div>
  );
}

export type Tone = "ok" | "pending" | "bad" | "none";

export function Dot({ tone }: { tone: Tone }) {
  // Shape differs per tone so state never depends on colour alone.
  const c = {
    ok: "rounded-full bg-ok",
    pending: "rounded-full border-2 border-pending",
    bad: "rotate-45 rounded-[1px] bg-danger scale-90",
    none: "rounded-full border-[1.5px] border-muted/70",
  }[tone];
  return <span aria-hidden className={`inline-block h-2.5 w-2.5 shrink-0 ${c}`} />;
}

/** Key/value row. `state` makes unavailable/pending values explicit. */
export function Row({
  label,
  children,
  state = "confirmed",
}: {
  label: string;
  children?: ReactNode;
  state?: "confirmed" | "pending" | "unavailable";
}) {
  return (
    <div className="flex items-baseline justify-between gap-4 border-b border-line py-2 last:border-b-0">
      <dt className="shrink-0 text-muted">{label}</dt>
      <dd className="min-w-0 truncate text-right">
        {state === "unavailable" ? (
          <span className="text-muted">{children ?? "Unavailable"}</span>
        ) : state === "pending" ? (
          <span className="text-pending">{children ?? "Pending"}</span>
        ) : (
          children
        )}
      </dd>
    </div>
  );
}

export function Panel({
  title,
  help,
  actions,
  children,
  className = "",
}: {
  help?: HelpKey;
  title?: string;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={className}>
      {title && (
        <header className="mb-2 flex min-h-7 items-center justify-between gap-3 px-1">
          <h2 className="text-base font-bold">
            {title}
            {help && <InfoTip topic={help} />}
          </h2>
          {actions}
        </header>
      )}
      <div className="rounded-xl bg-surface px-6 py-2 shadow-(--shadow)">{children}</div>
    </section>
  );
}

/** Native <dialog>: focus trap, Escape and inert background for free. */
export function Modal({
  open,
  onClose,
  title,
  children,
  wide,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  wide?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal?.();
    if (!open && d.open) d.close?.();
  }, [open]);
  const titleId = useId();
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: Escape already closes a native dialog; the click only dismisses on the backdrop.
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      onClose={onClose}
      onClick={(e) => {
        // A click on the backdrop lands on the <dialog> itself, not on its content.
        if (e.target === e.currentTarget) onClose();
      }}
      className={`m-auto max-h-[85vh] w-[calc(100%-32px)] overflow-auto rounded-2xl border border-line bg-surface p-0 text-fg shadow-[0_24px_60px_-20px_rgb(10_16_22/0.45)] ${
        wide ? "max-w-3xl" : "max-w-md"
      }`}
    >
      {open && (
        <>
          <header className="sticky top-0 z-10 flex items-center justify-between gap-3 border-b border-line bg-surface px-5 py-3.5">
            <h2 id={titleId} className="text-[17px] font-semibold">
              {title}
            </h2>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close"
              className="grid size-8 shrink-0 place-items-center rounded-full bg-raised text-muted hover:text-fg"
            >
              <X size={16} aria-hidden />
            </button>
          </header>
          <div className="px-5 py-4">{children}</div>
        </>
      )}
    </dialog>
  );
}

export function Confirm({
  open,
  title,
  body,
  confirmLabel,
  danger,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  title: string;
  body: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <Modal open={open} onClose={onCancel} title={title}>
      <div className="text-[13px] leading-relaxed">{body}</div>
      <div className="mt-5 flex justify-end gap-2">
        <Button onClick={onCancel} autoFocus>
          Cancel
        </Button>
        <Button variant={danger ? "danger" : "primary"} onClick={onConfirm}>
          {confirmLabel}
        </Button>
      </div>
    </Modal>
  );
}

/** Segmented choice built on native radios, so arrow keys and screen readers work as expected. */
export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
  hideLabel,
}: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  hideLabel?: boolean;
}) {
  const name = useId();
  return (
    <fieldset>
      <legend className={hideLabel ? "sr-only" : "mb-1 text-[13px] font-medium"}>{label}</legend>
      <div className="flex flex-wrap gap-1.5">
        {options.map((o) => (
          <label key={o.value} className="cursor-pointer">
            <input
              type="radio"
              name={name}
              value={o.value}
              checked={value === o.value}
              onChange={() => onChange(o.value)}
              className="peer sr-only"
            />
            <span className="block rounded-full bg-chip px-3.5 py-1.5 text-[12.5px] font-semibold text-muted peer-checked:bg-accent peer-checked:text-accent-fg peer-focus-visible:outline-2 peer-focus-visible:outline-focus hover:text-fg">
              {o.label}
            </span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}

/** Bar above a list: select-all checkbox, a count, and a bulk action. */
export function SelectBar({
  total,
  selected,
  onToggleAll,
  onClear,
  onDelete,
  disabled,
}: {
  total: number;
  selected: number;
  onToggleAll: () => void;
  onClear: () => void;
  onDelete: () => void;
  disabled?: boolean;
}) {
  const id = useId();
  const all = total > 0 && selected === total;
  return (
    <div className="flex flex-wrap items-center gap-3 rounded-[10px] bg-surface px-4 py-2 shadow-(--shadow)">
      <input
        id={id}
        type="checkbox"
        checked={all}
        ref={(el) => {
          if (el) el.indeterminate = selected > 0 && !all;
        }}
        onChange={onToggleAll}
        disabled={total === 0}
      />
      <label htmlFor={id} className="text-[13px] font-medium">
        {selected > 0 ? `${selected} selected` : "Select all"}
      </label>
      <InfoTip topic="bulkDelete" />
      {selected > 0 && (
        <div className="ml-auto flex gap-2">
          <Button onClick={onClear}>Clear</Button>
          <Button variant="danger" disabled={disabled} onClick={onDelete}>
            Delete selected ({selected})
          </Button>
        </div>
      )}
    </div>
  );
}
