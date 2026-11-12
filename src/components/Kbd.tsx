/** A keyboard shortcut hint, e.g. <Kbd keys={["⌘", "K"]} />. */
export function Kbd({ keys }: { keys: string[] }) {
  return (
    <span className="kbd-group" aria-hidden>
      {keys.map((k) => (
        <kbd key={k}>{k}</kbd>
      ))}
    </span>
  );
}
