type CheckboxProps = {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
  label?: React.ReactNode;
  description?: React.ReactNode;
  "aria-label"?: string;
};

   
                                                              
                                                                 
   
export default function Checkbox({
  checked,
  onChange,
  disabled = false,
  className = "",
  label,
  description,
  "aria-label": ariaLabel,
}: CheckboxProps) {
  const mark = (
    <span className={`signum-check ${checked ? "is-on" : ""} ${disabled ? "is-disabled" : ""}`.trim()} aria-hidden>
      <span className="signum-check-box">
        <svg className="signum-check-mark" viewBox="0 0 16 16" fill="none">
          <path
            d="M3.5 8.2 6.6 11.3 12.5 4.7"
            stroke="currentColor"
            strokeWidth="2.2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </span>
    </span>
  );

  if (!label && !description) {
    return (
      <button
        type="button"
        role="checkbox"
        aria-checked={checked}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => {
          if (!disabled) onChange(!checked);
        }}
        className={className}
      >
        {mark}
      </button>
    );
  }

  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={() => {
        if (!disabled) onChange(!checked);
      }}
      className={`signum-check-row group ${disabled ? "is-disabled" : ""} ${className}`.trim()}
    >
      {mark}
      <span className="min-w-0 flex-1 text-left">
        {label && (
          <span className="block text-sm font-medium text-theme">{label}</span>
        )}
        {description && (
          <span className="mt-0.5 block text-xs leading-4 text-muted">{description}</span>
        )}
      </span>
    </button>
  );
}
