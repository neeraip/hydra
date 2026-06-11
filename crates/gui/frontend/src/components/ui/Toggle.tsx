interface ToggleProps {
  checked: boolean;
  onChange: (value: boolean) => void;
  /** Tooltip shown on the label element. */
  title?: string;
  /** Associates the hidden input with an external label via htmlFor. */
  id?: string;
}

export function Toggle({ checked, onChange, title, id }: ToggleProps) {
  return (
    <label className="toggle" data-tooltip={title}>
      <input
        id={id}
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="toggle-track" />
      <span className="toggle-thumb" />
    </label>
  );
}
