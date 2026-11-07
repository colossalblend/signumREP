type ToggleSwitchProps = {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
                                                                 
  "aria-label"?: string;
};

   
                                                            
   
export default function ToggleSwitch({
  checked,
  onChange,
  disabled = false,
  className = "",
  "aria-label": ariaLabel,
}: ToggleSwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={e => {
        e.preventDefault();
        e.stopPropagation();
        if (!disabled) onChange(!checked);
      }}
      className={`signum-switch ${checked ? "is-on" : ""} ${disabled ? "is-disabled" : ""} ${className}`.trim()}
    >
      <span className="signum-switch-track" aria-hidden>
        <span className="signum-switch-thumb" />
      </span>
    </button>
  );
}
