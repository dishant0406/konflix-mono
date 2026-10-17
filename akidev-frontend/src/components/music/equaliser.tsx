/** Three bars that move while a track plays and stop when it is paused. */
export default function Equaliser({ paused }: { paused: boolean }) {
  return (
    <span className="eq flex h-3.5 items-end gap-0.5" data-paused={paused} aria-hidden>
      <i />
      <i />
      <i />
    </span>
  );
}
