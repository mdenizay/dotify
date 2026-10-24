import { useEditor } from "../state/editor";

export function StatusBar() {
  const cursor = useEditor((s) => s.cursor);
  const width = useEditor((s) => s.session?.summary.width ?? 0);
  const height = useEditor((s) => s.session?.summary.height ?? 0);
  const layerName = useEditor(
    (s) => s.session?.summary.layers.find((l) => l.id === s.activeLayerId)?.name ?? "",
  );
  const toast = useEditor((s) => s.toast);
  const inside = cursor && cursor.x >= 0 && cursor.y >= 0 && cursor.x < width && cursor.y < height;

  return (
    <footer className="flex h-6 shrink-0 items-center gap-4 border-t border-line bg-panel px-3 font-mono text-[11px] text-faint tabular-nums">
      <span>
        {width}×{height}
      </span>
      <span className="w-20">{inside ? `${cursor.x}, ${cursor.y}` : "–"}</span>
      <span className="flex w-28 items-center gap-1.5">
        {inside && cursor.color && (
          <>
            <span className="checker inline-block h-2.5 w-2.5 rounded-[2px]">
              <span
                className="block h-full w-full rounded-[2px]"
                style={{ background: cursor.color }}
              />
            </span>
            {cursor.color}
          </>
        )}
      </span>
      <span className="truncate font-sans">{layerName}</span>
      <div className="flex-1" />
      {toast && (
        <span
          role="status"
          className={`truncate font-sans ${toast.kind === "error" ? "text-accent" : "text-dim"}`}
        >
          {toast.message}
        </span>
      )}
    </footer>
  );
}
