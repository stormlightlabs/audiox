import { createSignal, createUniqueId, type ParentProps, Show } from "solid-js";
import { Motion, Presence } from "solid-motionone";

type AccordionProps = ParentProps & {
  title: string;
  summary?: string;
  defaultOpen?: boolean;
  id?: string;
  class?: string;
  headerClass?: string;
  contentClass?: string;
};

export function Accordion(props: AccordionProps) {
  const [isOpen, setIsOpen] = createSignal(props.defaultOpen ?? false);
  const baseId = () => props.id ?? createUniqueId();
  const buttonId = () => `${baseId()}-trigger`;
  const panelId = () => `${baseId()}-panel`;

  return (
    <section class={`w-full ${props.class ?? ""}`}>
      <button
        id={buttonId()}
        type="button"
        class={`flex w-full items-center justify-between gap-3 ${props.headerClass ?? ""}`}
        aria-expanded={isOpen()}
        aria-controls={panelId()}
        onClick={() => {
          setIsOpen((open) => !open);
        }}>
        <span class="text-left">
          <span class="block text-sm font-semibold text-text">{props.title}</span>
          <Show when={props.summary}>
            {(summary) => <span class="mt-0.5 block text-xs text-subtext">{summary()}</span>}
          </Show>
        </span>
        <Motion.span
          aria-hidden="true"
          class="text-lg leading-none text-subtext transition-transform duration-200"
          initial={{ rotate: isOpen() ? 90 : 0 }}
          animate={{ rotate: isOpen() ? 90 : 0 }}
          transition={{ duration: 0.2 }}>
          ›
        </Motion.span>
      </button>

      <Presence>
        <Show when={isOpen()}>
          <Motion.div
            id={panelId()}
            role="region"
            aria-labelledby={buttonId()}
            class="overflow-hidden"
            initial={{ opacity: 0, height: 0, y: -4 }}
            animate={{ opacity: 1, height: "auto", y: 0 }}
            exit={{ opacity: 0, height: 0, y: -4 }}
            transition={{ duration: 0.22 }}>
            <div class={props.contentClass}>{props.children}</div>
          </Motion.div>
        </Show>
      </Presence>
    </section>
  );
}
