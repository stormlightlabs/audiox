export function ProgressBar(props: { percent: number }) {
  return (
    <div class="h-2 overflow-hidden rounded-full border border-overlay bg-surface/50">
      <div
        class="h-full rounded-full bg-accent/75 transition-[width] duration-200"
        style={{ width: `${Math.max(0, Math.min(100, props.percent))}%` }} />
    </div>
  );
}
