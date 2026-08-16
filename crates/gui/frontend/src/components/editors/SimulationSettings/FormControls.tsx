import { useEffect, useState } from "react";

const inputStyle: React.CSSProperties = {
  background: "var(--bg-panel)",
  border: "1px solid var(--border)",
  borderRadius: 4,
  padding: "5px 8px",
  fontSize: "var(--text-md)",
  color: "var(--text-primary)",
  fontFamily: "var(--font-ui)",
  width: "100%",
  outline: "none",
  boxSizing: "border-box",
};

export function FieldGrid({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
        gap: "10px 16px",
      }}
    >
      {children}
    </div>
  );
}

export function Field({
  label,
  help,
  editing,
  control,
  display = "",
}: {
  label: string;
  help?: string;
  editing: boolean;
  control: React.ReactNode;
  /** Read-only rendering; unused (and optional) when `editing` is true. */
  display?: string;
}) {
  return (
    <div
      style={{ display: "flex", flexDirection: "column", gap: 4, minWidth: 0 }}
    >
      <div
        style={{
          fontSize: "var(--text-sm)",
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: "0.05em",
        }}
      >
        {label}
      </div>
      {editing ? (
        control
      ) : (
        <div
          style={{
            fontSize: "var(--text-md)",
            color: "var(--text-primary)",
            fontFamily: "var(--font-ui)",
            padding: "5px 0",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
          data-tooltip={help}
        >
          {display}
        </div>
      )}
    </div>
  );
}

export function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div style={{ color: "var(--text-tertiary)", fontSize: "var(--text-md)" }}>
      {children}
    </div>
  );
}

export function NumberInput({
  value,
  onChange,
  step,
  min,
  max,
}: {
  value: number;
  onChange: (v: number) => void;
  step?: number;
  min?: number;
  max?: number;
}) {
  const [local, setLocal] = useState(String(value));
  useEffect(() => {
    setLocal(String(value));
  }, [value]);
  return (
    <input
      type="number"
      value={local}
      step={step}
      min={min}
      max={max}
      onChange={(e) => setLocal(e.target.value)}
      onBlur={() => {
        const n = parseFloat(local);
        if (Number.isFinite(n)) onChange(n);
        else setLocal(String(value));
      }}
      style={inputStyle}
    />
  );
}

export function HoursInput({
  value,
  onChange,
}: {
  value: number;
  onChange: (seconds: number) => void;
}) {
  return (
    <NumberInput
      value={value / 3600}
      onChange={(h) => onChange(Math.max(0, h) * 3600)}
      step={0.5}
      min={0}
    />
  );
}

export function MinutesInput({
  value,
  onChange,
}: {
  value: number;
  onChange: (seconds: number) => void;
}) {
  return (
    <NumberInput
      value={value / 60}
      onChange={(m) => onChange(Math.max(0, m) * 60)}
      step={1}
      min={0}
    />
  );
}

export function TimeInput({
  value,
  onChange,
}: {
  value: number;
  onChange: (seconds: number) => void;
}) {
  const hh = Math.floor(value / 3600) % 24;
  const mm = Math.floor((value % 3600) / 60);
  const formatted = `${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}`;
  return (
    <input
      type="time"
      value={formatted}
      onChange={(e) => {
        const [h, m] = e.target.value.split(":").map(Number);
        if (Number.isFinite(h) && Number.isFinite(m))
          onChange(h * 3600 + m * 60);
      }}
      style={inputStyle}
    />
  );
}

export function TextInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  return (
    <input
      type="text"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      style={inputStyle}
    />
  );
}

export function Select<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string }[];
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value as T)}
      style={inputStyle}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}
