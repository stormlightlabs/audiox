import type { ParentProps } from "solid-js";

type ViewScaffoldProps = ParentProps & { eyebrow: string; title: string; description: string };

export function ViewScaffold(props: ViewScaffoldProps) {
  return (
    <section class="space-y-8">
      <header class="space-y-3">
        <p class="text-xs font-semibold tracking-[0.2em] text-subtext uppercase">{props.eyebrow}</p>
        <h1 class="font-display text-4xl leading-tight text-text">{props.title}</h1>
        <p class="max-w-3xl text-base leading-relaxed text-subtext">{props.description}</p>
      </header>
      {props.children}
    </section>
  );
}
