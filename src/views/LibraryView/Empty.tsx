import { A } from "@solidjs/router";

export function NoMatchesFound() {
  return (
    <p class="rounded-xl border border-overlay bg-elevation/65 p-3 text-sm text-subtext">
      No matching chunks found for this query.
    </p>
  );
}

export function EmptyState() {
  return (
    <section class="rounded-2xl border border-dashed border-overlay bg-surface/35 p-6">
      <p class="text-base font-semibold text-text">No matching documents yet</p>
      <p class="mt-1 text-sm text-subtext">
        Import audio, record from your microphone, or import a text note to create your first document.
      </p>
      <div class="mt-4 flex flex-wrap gap-2">
        <A
          href="/import"
          class="rounded-xl bg-accent px-3 py-1.5 text-xs font-semibold text-surface transition hover:brightness-110">
          Open Import
        </A>
        <A
          href="/record"
          class="rounded-xl border border-overlay px-3 py-1.5 text-xs font-semibold text-subtext transition hover:border-accent/35 hover:text-text">
          Open Record
        </A>
      </div>
    </section>
  );
}
