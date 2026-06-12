import { useEffect, useRef, useState } from "react";
import type { Project } from "../../hooks";

export interface ContextMenuState {
  project: Project;
  x: number;
  y: number;
}

export function ContextMenu({
  menu,
  onClose,
  onOpen,
  onOpenFolder,
  onRemove,
  onRename,
}: {
  menu: ContextMenuState;
  onClose: () => void;
  onOpen: (id: string) => void;
  onOpenFolder: (id: string) => void;
  onRemove: (id: string) => void;
  onRename: (id: string, name: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [renaming, setRenaming] = useState(false);
  const [newName, setNewName] = useState(menu.project.name);
  const nameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (renaming && nameInputRef.current) {
      nameInputRef.current.focus();
      nameInputRef.current.select();
    }
  }, [renaming]);

  function handleConfirmRename() {
    const name = newName.trim();
    if (name && name !== menu.project.name) onRename(menu.project.id, name);
    onClose();
  }

  useEffect(() => {
    function dismiss(e: MouseEvent | KeyboardEvent) {
      if (e instanceof KeyboardEvent) {
        if (e.key === "Escape") onClose();
        return;
      }
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    }
    window.addEventListener("mousedown", dismiss);
    window.addEventListener("keydown", dismiss);
    return () => {
      window.removeEventListener("mousedown", dismiss);
      window.removeEventListener("keydown", dismiss);
    };
  }, [onClose]);

  const folderMissing = menu.project.folderMissing;
  const MENU_W = 180,
    MENU_H = folderMissing ? 44 : 116;
  const x = Math.min(menu.x, window.innerWidth - MENU_W - 8);
  const y = Math.min(menu.y, window.innerHeight - MENU_H - 8);

  const itemStyle: React.CSSProperties = {
    display: "block",
    width: "100%",
    padding: "7px 14px",
    border: "none",
    background: "transparent",
    textAlign: "left",
    fontSize: 13,
    fontFamily: "var(--font-ui)",
    color: "var(--text-primary)",
    cursor: "pointer",
    borderRadius: 4,
  };

  return (
    <div
      ref={ref}
      style={{
        position: "fixed",
        top: y,
        left: x,
        zIndex: 500,
        background: "var(--bg-panel)",
        border: "1px solid var(--border-hover)",
        borderRadius: 8,
        boxShadow: "var(--shadow-3)",
        padding: 4,
        minWidth: MENU_W,
      }}
    >
      {folderMissing ? (
        <button
          type="button"
          style={{ ...itemStyle, color: "var(--status-error)" }}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLButtonElement).style.background =
              "var(--bg-card)";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLButtonElement).style.background =
              "transparent";
          }}
          onClick={() => {
            onRemove(menu.project.id);
            onClose();
          }}
        >
          Remove from list
        </button>
      ) : (
        <>
          <button
            type="button"
            style={itemStyle}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "var(--bg-card)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "transparent";
            }}
            onClick={() => {
              onOpen(menu.project.id);
              onClose();
            }}
          >
            Open
          </button>
          <button
            type="button"
            style={itemStyle}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "var(--bg-card)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "transparent";
            }}
            onClick={() => {
              onOpenFolder(menu.project.id);
              onClose();
            }}
          >
            Open folder
          </button>
          {renaming ? (
            <div style={{ padding: "4px 8px" }}>
              <input
                ref={nameInputRef}
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleConfirmRename();
                  if (e.key === "Escape") onClose();
                }}
                onBlur={handleConfirmRename}
                style={{
                  width: "100%",
                  boxSizing: "border-box",
                  padding: "4px 8px",
                  border: "1px solid var(--border-focus)",
                  borderRadius: 4,
                  background: "var(--bg-input)",
                  color: "var(--text-primary)",
                  fontSize: 13,
                  fontFamily: "var(--font-ui)",
                  outline: "none",
                }}
              />
            </div>
          ) : (
            <button
              type="button"
              style={itemStyle}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "var(--bg-card)";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "transparent";
              }}
              onClick={() => setRenaming(true)}
            >
              Rename…
            </button>
          )}
        </>
      )}
    </div>
  );
}
