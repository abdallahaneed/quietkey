// Shared UI primitives on Radix. Colours come from the token classes only, never raw hex.
import { forwardRef, useId, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";
import { Dialog, DropdownMenu, RadioGroup, Select, Switch as RSwitch, Tooltip } from "radix-ui";
import { Check, ChevronDown, Loader2, Lock } from "lucide-react";

const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(" ");

/* Button ------------------------------------------------------------------ */

type Variant = "primary" | "secondary" | "ghost" | "danger";
type Size = "sm" | "md" | "lg";
const variants: Record<Variant, string> = {
  primary: "bg-accent text-on-accent hover:brightness-110",
  secondary: "bg-bg text-fg border border-border hover:bg-surface",
  ghost: "bg-transparent text-fg hover:bg-surface-2",
  danger: "bg-danger text-on-accent hover:brightness-110",
};
const sizes: Record<Size, string> = { sm: "h-7 px-2.5 text-sm", md: "h-8 px-3 text-base", lg: "h-10 px-4 text-base" };

export const Button = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: Size; loading?: boolean; icon?: ReactNode }
>(({ variant = "secondary", size = "md", loading, icon, className, children, disabled, ...rest }, ref) => (
  <button
    ref={ref}
    {...rest}
    disabled={disabled || loading}
    aria-busy={loading || undefined}
    className={cx(
      "inline-flex items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap",
      "transition-[background,filter] duration-[var(--motion-fast)] active:translate-y-px",
      "disabled:opacity-40 disabled:pointer-events-none",
      variants[variant], sizes[size], className,
    )}
  >
    {loading ? <Loader2 className="size-4 animate-spin" aria-hidden /> : icon}
    {children}
  </button>
));

export function IconButton({ label, children, className, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <Tooltip.Provider delayDuration={400}>
      <Tooltip.Root>
        <Tooltip.Trigger asChild>
          <button
            aria-label={label}
            {...rest}
            className={cx("inline-flex size-7 items-center justify-center rounded-md text-muted hover:bg-surface-2 hover:text-fg", className)}
          >
            {children}
          </button>
        </Tooltip.Trigger>
        <Tooltip.Portal>
          <Tooltip.Content sideOffset={4} className="rounded-sm bg-fg px-2 py-1 text-xs text-bg shadow-pop">
            {label}
          </Tooltip.Content>
        </Tooltip.Portal>
      </Tooltip.Root>
    </Tooltip.Provider>
  );
}

/* Text input --------------------------------------------------------------- */

export function TextInput({
  label, hideLabel, error, max, value, className, ...rest
}: InputHTMLAttributes<HTMLInputElement> & { label: string; hideLabel?: boolean; error?: string; max?: number }) {
  const id = useId();
  const len = typeof value === "string" ? value.length : 0;
  return (
    <div className={cx("flex flex-col gap-1", className)}>
      <div className="flex items-baseline justify-between">
        <label htmlFor={id} className={cx("text-sm font-medium", hideLabel && "sr-only")}>{label}</label>
        {max != null && (
          <span className={cx("text-xs", len > max ? "text-danger" : "text-muted")} aria-live={len >= max * 0.8 ? "polite" : "off"}>
            {len}/{max}
          </span>
        )}
      </div>
      <input
        id={id}
        value={value}
        aria-invalid={!!error || undefined}
        aria-describedby={error ? `${id}-err` : undefined}
        {...rest}
        className={cx(
          "h-8 rounded-md border bg-bg px-2.5 text-base text-fg placeholder:text-muted",
          error ? "border-danger" : "border-input focus:border-accent",
        )}
      />
      {error && <p id={`${id}-err`} className="text-sm text-danger">{error}</p>}
    </div>
  );
}

/* Switch ------------------------------------------------------------------- */

export function Switch({
  label, description, checked, onChange, disabled, lockedReason,
}: { label: string; description?: string; checked: boolean; onChange: (v: boolean) => void; disabled?: boolean; lockedReason?: string }) {
  const id = useId();
  return (
    <div className="flex items-start justify-between gap-6 py-3">
      <div className="flex flex-col gap-0.5">
        <label htmlFor={id} className="flex items-center gap-1.5 text-base font-medium">
          {label}
          {lockedReason && <Lock className="size-3.5 text-muted" aria-label={lockedReason} />}
        </label>
        {description && <p className="text-sm text-muted">{description}</p>}
      </div>
      <RSwitch.Root
        id={id}
        checked={checked}
        onCheckedChange={onChange}
        disabled={disabled || !!lockedReason}
        className="relative h-5 w-9 shrink-0 rounded-pill bg-surface-2 border border-border transition-colors data-[state=checked]:bg-accent data-[state=checked]:border-accent disabled:opacity-40"
      >
        <RSwitch.Thumb className="block size-4 translate-x-0.5 rounded-pill bg-white shadow-card transition-transform data-[state=checked]:translate-x-[17px]" />
      </RSwitch.Root>
    </div>
  );
}

/* Radio cards -------------------------------------------------------------- */

export function RadioCards<T extends string>({
  label, value, onChange, options,
}: { label: string; value: T; onChange: (v: T) => void; options: { value: T; title: string; body?: string; disabled?: boolean }[] }) {
  return (
    <RadioGroup.Root aria-label={label} value={value} onValueChange={(v) => onChange(v as T)} className="grid gap-2">
      {options.map((o) => (
        <RadioGroup.Item
          key={o.value}
          value={o.value}
          disabled={o.disabled}
          className="group flex items-start gap-3 rounded-lg border border-border bg-bg p-4 text-left hover:bg-surface data-[state=checked]:border-accent data-[state=checked]:ring-1 data-[state=checked]:ring-accent disabled:opacity-40"
        >
          <span className="mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-pill border border-input group-data-[state=checked]:border-accent group-data-[state=checked]:bg-accent">
            <RadioGroup.Indicator><Check className="size-3 text-on-accent" aria-hidden /></RadioGroup.Indicator>
          </span>
          <span className="flex flex-col gap-0.5">
            <span className="text-base font-semibold">{o.title}</span>
            {o.body && <span className="text-sm text-muted">{o.body}</span>}
          </span>
        </RadioGroup.Item>
      ))}
    </RadioGroup.Root>
  );
}

/* Select ------------------------------------------------------------------- */

export function SelectMenu<T extends string>({
  label, value, onChange, options,
}: { label: string; value: T; onChange: (v: T) => void; options: { value: T; label: string }[] }) {
  return (
    <Select.Root value={value} onValueChange={(v) => onChange(v as T)}>
      <Select.Trigger aria-label={label} className="inline-flex h-8 items-center gap-2 rounded-md border border-border bg-bg px-3 text-base hover:bg-surface">
        <Select.Value />
        <Select.Icon><ChevronDown className="size-4 text-muted" aria-hidden /></Select.Icon>
      </Select.Trigger>
      <Select.Portal>
        <Select.Content position="popper" sideOffset={4} className="z-50 min-w-36 rounded-md border border-border bg-bg p-1 shadow-pop">
          <Select.Viewport>
            {options.map((o) => (
              <Select.Item key={o.value} value={o.value} className="flex h-7 items-center justify-between gap-4 rounded-sm px-2 text-base outline-none data-[highlighted]:bg-accent-soft">
                <Select.ItemText>{o.label}</Select.ItemText>
                <Select.ItemIndicator><Check className="size-4 text-accent" aria-hidden /></Select.ItemIndicator>
              </Select.Item>
            ))}
          </Select.Viewport>
        </Select.Content>
      </Select.Portal>
    </Select.Root>
  );
}

/* Modal -------------------------------------------------------------------- */

export function Modal({
  open, onOpenChange, title, description, children, footer, width = "w-[440px]",
}: { open: boolean; onOpenChange: (o: boolean) => void; title: string; description?: string; children?: ReactNode; footer?: ReactNode; width?: string }) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/30" />
        <Dialog.Content className={cx("fixed left-1/2 top-1/2 z-50 max-w-[calc(100vw-32px)] -translate-x-1/2 -translate-y-1/2 rounded-lg bg-bg p-5 shadow-pop", width)}>
          <Dialog.Title className="text-lg font-semibold">{title}</Dialog.Title>
          {description ? <Dialog.Description className="mt-1 text-sm text-muted">{description}</Dialog.Description> : <Dialog.Description className="sr-only">{title}</Dialog.Description>}
          <div className="mt-4">{children}</div>
          {footer && <div className="mt-5 flex justify-end gap-2">{footer}</div>}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/* Dropdown menu ------------------------------------------------------------ */

export function Menu({ trigger, items }: { trigger: ReactNode; items: { label: string; onSelect: () => void; danger?: boolean; disabled?: boolean }[] }) {
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>{trigger}</DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content align="end" sideOffset={4} className="z-50 min-w-40 rounded-md border border-border bg-bg p-1 shadow-pop">
          {items.map((i) => (
            <DropdownMenu.Item
              key={i.label}
              disabled={i.disabled}
              onSelect={i.onSelect}
              className={cx("flex h-7 items-center rounded-sm px-2 text-base outline-none data-[highlighted]:bg-accent-soft data-[disabled]:opacity-40", i.danger && "text-danger")}
            >
              {i.label}
            </DropdownMenu.Item>
          ))}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

/* Small pieces --------------------------------------------------------------- */

const KEY_GLYPH: Record<string, string> = { Ctrl: "⌃", Opt: "⌥", Cmd: "⌘", Shift: "⇧", Fn: "fn", Space: "space", Escape: "esc" };

export function KeyCaps({ keys }: { keys: string[] }) {
  return (
    <span className="inline-flex gap-1" aria-label={keys.join(" plus ")}>
      {keys.map((k) => (
        <kbd key={k} className="min-w-6 rounded-sm border border-border bg-bg px-1.5 py-0.5 text-center font-mono text-xs text-fg shadow-card">
          {KEY_GLYPH[k] ?? k}
        </kbd>
      ))}
    </span>
  );
}

export function PageHeader({ title, description, actions }: { title: string; description?: string; actions?: ReactNode }) {
  return (
    <header className="mb-6 flex items-end justify-between gap-4">
      <div>
        <h1 className="text-xl font-bold">{title}</h1>
        {description && <p className="mt-1 text-sm text-muted">{description}</p>}
      </div>
      {actions && <div className="flex gap-2">{actions}</div>}
    </header>
  );
}

export function EmptyState({ icon, title, body, action }: { icon: ReactNode; title: string; body: string; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border px-6 py-12 text-center">
      <div className="mb-1 text-muted" aria-hidden>{icon}</div>
      <p className="text-base font-semibold">{title}</p>
      <p className="max-w-80 text-sm text-muted">{body}</p>
      {action && <div className="mt-3">{action}</div>}
    </div>
  );
}

export function StatTile({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="rounded-lg border border-border bg-bg p-4">
      <p className="text-sm text-muted">{label}</p>
      <p className="mt-1 text-display font-bold">{value}</p>
      {sub && <p className="mt-0.5 text-sm text-muted">{sub}</p>}
    </div>
  );
}

export function Notice({ tone = "info", children, action }: { tone?: "info" | "warning" | "error" | "success"; children: ReactNode; action?: ReactNode }) {
  const color = { info: "text-fg", warning: "text-warning", error: "text-danger", success: "text-success" }[tone];
  return (
    <div role={tone === "error" ? "alert" : "status"} className={cx("flex items-center justify-between gap-4 rounded-md border border-border bg-surface px-3 py-2 text-sm", color)}>
      <span>{children}</span>
      {action}
    </div>
  );
}

export function PermissionRow({ title, body, granted, onAllow }: { title: string; body: string; granted: boolean; onAllow: () => void }) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg bg-surface p-4" aria-live="polite">
      <div>
        <p className="text-base font-semibold">{title}</p>
        <p className="text-sm text-muted">{body}</p>
      </div>
      {granted ? (
        <span className="inline-flex items-center gap-1 text-sm font-medium text-success"><Check className="size-4" aria-hidden /> Allowed</span>
      ) : (
        <Button onClick={onAllow}>Allow</Button>
      )}
    </div>
  );
}

export function SkeletonRows({ n = 3 }: { n?: number }) {
  return (
    <div className="flex flex-col gap-3" role="status" aria-busy="true" aria-label="Loading">
      {Array.from({ length: n }, (_, i) => (
        <div key={i} className="h-11 animate-pulse rounded-md bg-surface-2" />
      ))}
    </div>
  );
}

export { cx };
