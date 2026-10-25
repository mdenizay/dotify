import { useState } from "react";
import { controller } from "../app/controller";
import { toRgbHex, uniqueTagName } from "../app/tags";
import type { AnimationTag, Direction, FrameId } from "../engine/types";
import { getEditor, selectFrames, selectTags, setEditor, useEditor } from "../state/editor";
import { Button, Dialog, inputClass, plainText } from "./ui/primitives";

export function TagDialog() {
  const dialog = useEditor((s) => s.tagDialog);
  const tags = useEditor(selectTags);
  if (!dialog) return null;
  if (dialog.mode === "create") {
    return <TagForm key="new" from={dialog.from} to={dialog.to} />;
  }
  const tag = tags.find((t) => t.name === dialog.tag);
  // Deleted or renamed elsewhere (e.g. undo) while open.
  if (!tag) return null;
  return <TagForm key={tag.name} tag={tag} from={tag.from} to={tag.to} />;
}

function TagForm({ tag, from, to }: { tag?: AnimationTag; from: FrameId; to: FrameId }) {
  const frames = useEditor(selectFrames);
  const tags = useEditor(selectTags);
  const [name, setName] = useState(tag?.name ?? uniqueTagName(tags));
  const [color, setColor] = useState(tag ? toRgbHex(tag.color) : null);
  const [direction, setDirection] = useState<Direction>(tag?.direction ?? "forward");
  const [loop, setLoop] = useState(tag?.loop ?? true);
  const [start, setStart] = useState(frames.findIndex((f) => f.id === from));
  const [end, setEnd] = useState(frames.findIndex((f) => f.id === to));
  const close = () => setEditor({ tagDialog: null });

  const trimmed = name.trim();
  const taken = tags.some((t) => t.name === trimmed && t !== tag);
  const valid = trimmed.length > 0 && !taken && start >= 0 && end >= start;

  const submit = () => {
    if (!valid) return;
    const fromId = frames[start]!.id;
    const toId = frames[end]!.id;
    const hex = color && `${color}ff`;
    close();
    if (!tag) {
      void controller.run({
        type: "create_tag",
        name: trimmed,
        from: fromId,
        to: toId,
        direction,
        loop,
        color: hex,
      });
      setEditor({ frameAnchor: null });
      return;
    }
    // Only send what changed, so the history label stays meaningful.
    void controller.run({
      type: "update_tag",
      tag: tag.name,
      name: trimmed !== tag.name ? trimmed : null,
      from: fromId !== tag.from ? fromId : null,
      to: toId !== tag.to ? toId : null,
      direction: direction !== tag.direction ? direction : null,
      loop: loop !== tag.loop ? loop : null,
      color: hex && hex !== tag.color ? hex : null,
    });
    if (trimmed !== tag.name && getEditor().playTag === tag.name) setEditor({ playTag: trimmed });
  };

  const remove = () => {
    if (!tag) return;
    close();
    void controller.run({ type: "delete_tag", tag: tag.name });
  };

  const frameOptions = frames.map((_, i) => (
    <option key={i} value={i}>
      {i + 1}
    </option>
  ));

  return (
    <Dialog title={tag ? "Tag Properties" : "New Tag"} onClose={close} width={320}>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <div className="flex items-end gap-2">
          <label className="flex flex-1 flex-col gap-1 text-dim">
            Name
            <input
              {...plainText}
              className={inputClass}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <label className="flex flex-col gap-1 text-dim">
            Color
            <input
              type="color"
              aria-label="Tag color"
              value={color ?? "#5b8cff"}
              onChange={(e) => setColor(e.target.value)}
              className="h-7 w-9 cursor-pointer rounded-[5px] border border-line-strong bg-panel p-0.5"
            />
          </label>
        </div>
        <div className="flex gap-2">
          <label className="flex flex-1 flex-col gap-1 text-dim">
            From frame
            <select
              className={inputClass}
              value={start}
              onChange={(e) => setStart(Number(e.target.value))}
            >
              {frameOptions}
            </select>
          </label>
          <label className="flex flex-1 flex-col gap-1 text-dim">
            To frame
            <select
              className={inputClass}
              value={end}
              onChange={(e) => setEnd(Number(e.target.value))}
            >
              {frameOptions}
            </select>
          </label>
        </div>
        <div className="flex items-end gap-2">
          <label className="flex flex-1 flex-col gap-1 text-dim">
            Direction
            <select
              className={inputClass}
              value={direction}
              onChange={(e) => setDirection(e.target.value as Direction)}
            >
              <option value="forward">Forward</option>
              <option value="reverse">Reverse</option>
              <option value="ping_pong">Ping-pong</option>
            </select>
          </label>
          <label className="flex h-7 items-center gap-2 text-dim">
            <input type="checkbox" checked={loop} onChange={(e) => setLoop(e.target.checked)} />
            Loop
          </label>
        </div>
        {taken && <p className="text-accent">A tag named “{trimmed}” already exists.</p>}
        {end < start && (
          <p className="text-accent">The last frame must not come before the first.</p>
        )}
        <div className="flex items-center gap-2 pt-1">
          {tag && (
            <Button data-dismiss onClick={remove}>
              Delete
            </Button>
          )}
          <div className="flex-1" />
          <Button data-dismiss onClick={close}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" disabled={!valid}>
            {tag ? "Save" : "Create"}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
