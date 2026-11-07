import { Check, ChevronDown, Search } from "lucide-react";
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";

export type SelectMenuOption = {
  value: string;
  label: string;
  hint?: string;
  icon?: ReactNode;
  description?: string;
  disabled?: boolean;
};

type Props = {
  value: string;
  options: SelectMenuOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  placeholder?: string;
  searchPlaceholder?: string;
  className?: string;
  icon?: ReactNode;
  "aria-label"?: string;
};

const ROW = "flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm transition";

export default function SelectMenu({
  value,
  options,
  onChange,
  disabled,
  placeholder = "Выберите",
  searchPlaceholder = "Поиск…",
  className = "",
  icon,
  "aria-label": ariaLabel,
}: Props) {
  const menuId = useId();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [rect, setRect] = useState({ left: 0, top: 0, width: 0, up: false, maxHeight: 320 });
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popupRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const selected = options.find(o => o.value === value) ?? null;
  const searchable = options.length > 8;
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return options;
    return options.filter(o => `${o.label} ${o.hint ?? ""} ${o.description ?? ""}`.toLowerCase().includes(q));
  }, [options, query]);

  function place() {
    const box = triggerRef.current?.getBoundingClientRect();
    if (!box) return;
    const below = window.innerHeight - box.bottom - 12;
    const above = box.top - 12;
    const up = below < 240 && above > below;
    setRect({
      left: box.left,
      top: up ? box.top - 6 : box.bottom + 6,
      width: box.width,
      up,
      maxHeight: Math.max(180, Math.min(360, up ? above : below)),
    });
  }

  useLayoutEffect(() => {
    if (!open) return;
    place();
    const close = () => setOpen(false);
    // Прокрутка внутри самого списка не должна закрывать меню.
    const onScroll = (e: Event) => {
      if (popupRef.current?.contains(e.target as Node)) return;
      close();
    };
    window.addEventListener("resize", close);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      window.removeEventListener("resize", close);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const target = e.target as Node;
      if (popupRef.current?.contains(target) || triggerRef.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    const selectedIndex = options.findIndex(o => o.value === value && !o.disabled);
    setActive(selectedIndex >= 0 ? selectedIndex : options.findIndex(o => !o.disabled));
    if (!searchable) popupRef.current?.focus({ preventScroll: true });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    listRef.current?.querySelector(`[data-option-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [open, active, filtered.length]);

  function pick(option?: SelectMenuOption) {
    if (!option || option.disabled) return;
    setOpen(false);
    triggerRef.current?.focus();
    if (option.value !== value) onChange(option.value);
  }

  function onKeyDown(e: React.KeyboardEvent) {
    if (!open) {
      if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
        e.preventDefault();
        setOpen(true);
      }
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
      triggerRef.current?.focus();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      const next = filtered.findIndex((option, index) => index > active && !option.disabled);
      if (next >= 0) setActive(next);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      for (let index = active - 1; index >= 0; index--) {
        if (!filtered[index].disabled) { setActive(index); break; }
      }
    } else if (e.key === "Home") {
      e.preventDefault();
      setActive(filtered.findIndex(option => !option.disabled));
    } else if (e.key === "End") {
      e.preventDefault();
      for (let index = filtered.length - 1; index >= 0; index--) {
        if (!filtered[index].disabled) { setActive(index); break; }
      }
    } else if (e.key === "Tab") {
      setOpen(false);
    } else if (e.key === "Enter" || (e.key === " " && e.target === popupRef.current)) {
      e.preventDefault();
      pick(filtered[active]);
    }
  }

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        disabled={disabled}
        className={`field flex items-center justify-between gap-3 text-left disabled:cursor-not-allowed disabled:opacity-50 ${className}`}
        onClick={() => setOpen(v => !v)}
        onKeyDown={onKeyDown}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-label={ariaLabel}
      >
        {(selected?.icon || icon) && <span className="select-menu-icon">{selected?.icon ?? icon}</span>}
        <span className={`min-w-0 flex-1 truncate ${selected ? "" : "text-muted"}`}>
          {selected ? selected.label : placeholder}
          {selected?.hint && <span className="ml-2 text-xs text-muted">{selected.hint}</span>}
        </span>
        <ChevronDown
          size={16}
          className={`shrink-0 text-muted transition ${open ? "rotate-180" : ""}`}
        />
      </button>

      {open && !disabled && createPortal(
        <div
          ref={popupRef}
          role="listbox"
          id={menuId}
          tabIndex={-1}
          aria-label={ariaLabel ?? placeholder}
          aria-activedescendant={active >= 0 && filtered[active] ? `${menuId}-${active}` : undefined}
          className="select-menu-popup fixed z-[90] overflow-hidden rounded-2xl border border-subtle p-1.5 shadow-panel outline-none backdrop-blur-xl"
          style={{
            left: rect.left,
            width: rect.width,
            background: "var(--app-surface-strong)",
            transformOrigin: rect.up ? "bottom center" : "top center",
            ...(rect.up
              ? { bottom: window.innerHeight - rect.top, maxHeight: rect.maxHeight }
              : { top: rect.top, maxHeight: rect.maxHeight }),
          }}
          onKeyDown={onKeyDown}
        >
          <div className="flex max-h-full flex-col" style={{ maxHeight: rect.maxHeight - 12 }}>
            {searchable && (
              <div className="relative mb-1.5 shrink-0">
                <Search size={14} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted" />
                <input
                  autoFocus
                  className="w-full rounded-lg border border-subtle bg-black/10 py-2 pl-8 pr-3 text-sm text-theme outline-none placeholder:text-muted focus:border-brand-400/60"
                  placeholder={searchPlaceholder}
                  value={query}
                  onChange={e => {
                    setQuery(e.target.value);
                    setActive(0);
                  }}
                />
              </div>
            )}
            <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto">
              {filtered.length === 0 && (
                <div className="px-3 py-3 text-sm text-muted">Ничего не найдено</div>
              )}
              {filtered.map((option, index) => {
                const isSelected = option.value === value;
                return (
                  <button
                    key={option.value}
                    type="button"
                    role="option"
                    aria-selected={isSelected}
                    aria-disabled={option.disabled || undefined}
                    disabled={option.disabled}
                    id={`${menuId}-${index}`}
                    data-option-index={index}
                    tabIndex={-1}
                    className={`${ROW} ${index === active ? "bg-brand-500/15" : "hover:bg-brand-500/10"} ${
                      isSelected ? "text-brand-300" : "text-theme"
                    } disabled:cursor-not-allowed disabled:opacity-40`}
                    onMouseEnter={() => { if (!option.disabled) setActive(index); }}
                    onClick={() => pick(option)}
                  >
                    {option.icon && <span className="select-menu-icon">{option.icon}</span>}
                    <span className="min-w-0 flex-1">
                      <span className={`${option.description ? "block truncate" : ""} font-medium`}>{option.label}</span>
                      {option.description && <span className="mt-0.5 block truncate text-xs font-normal text-muted">{option.description}{option.hint ? ` · ${option.hint}` : ""}</span>}
                      {option.hint && !option.description && (
                        <span className="ml-2 text-xs text-muted">{option.hint}</span>
                      )}
                    </span>
                    {isSelected && <Check size={15} className="shrink-0" />}
                  </button>
                );
              })}
            </div>
          </div>
        </div>,
        document.body,
      )}
    </>
  );
}
