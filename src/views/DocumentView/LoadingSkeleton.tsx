export function LoadingSkeleton() {
  return (
    <div class="grid gap-3" aria-hidden="true">
      <div class="animate-pulse rounded-2xl border border-overlay bg-surface/35 p-4">
        <div class="h-4 w-1/4 rounded bg-overlay/70" />
        <div class="mt-3 h-3 w-10/12 rounded bg-overlay/60" />
        <div class="mt-2 h-3 w-8/12 rounded bg-overlay/60" />
      </div>
      <div class="animate-pulse rounded-2xl border border-overlay bg-surface/35 p-4">
        <div class="h-3 w-1/3 rounded bg-overlay/60" />
        <div class="mt-2 h-3 w-11/12 rounded bg-overlay/60" />
        <div class="mt-2 h-3 w-9/12 rounded bg-overlay/60" />
      </div>
    </div>
  );
}
