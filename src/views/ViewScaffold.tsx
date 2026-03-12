import type { ParentProps } from "solid-js";

type ViewScaffoldProps = ParentProps & { eyebrow: string; title: string; description: string };

export function ViewScaffold(props: ViewScaffoldProps) {
  return (
    <section class="space-y-8">
      <header class="space-y-4 rounded-3xl border border-overlay bg-surface/20 p-4 md:p-5">
        <p class="inline-flex w-fit rounded-full border border-overlay px-2.5 py-1 text-xs font-semibold tracking-[0.2em] text-subtext uppercase">
          {props.eyebrow}
        </p>
        <h1 class="font-display text-4xl leading-tight text-text">{props.title}</h1>
        <p class="max-w-3xl text-base leading-relaxed text-subtext">{props.description}</p>
      </header>
      {props.children}
    </section>
  );
}
