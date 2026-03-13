# SolidJS Reactivity Model

## Core Primitives

SolidJS uses fine-grained reactivity built on three primitives:

- **Signals** (`createSignal`): atomic reactive values. Reading a signal inside a tracking scope registers a dependency; writing triggers re-execution of all subscribers. Signals return a `[getter, setter]` tuple — the getter is a function call, not a property access.
- **Memos** (`createMemo`): derived computations that cache their result and only re-run when upstream signals change. Memos are themselves signals.
- **Effects** (`createEffect`): side-effects that re-run whenever their tracked dependencies change. Effects run asynchronously after the current synchronous batch completes.

## Compilation Strategy

SolidJS compiles JSX to real DOM operations at build time — there is no virtual DOM or diffing step. Each JSX expression compiles to a `createEffect` that updates the specific DOM node when its reactive dependency changes. This yields O(1) update cost per changed value regardless of component tree depth.

## Comparison with React

| Concern            | React                             | SolidJS                           |
| ------------------ | --------------------------------- | --------------------------------- |
| Re-render unit     | Entire component subtree          | Individual DOM node               |
| State primitive    | `useState` (triggers re-render)   | `createSignal` (triggers effect)  |
| Memoization        | `useMemo` / `React.memo` (opt-in) | Automatic via dependency tracking |
| Component function | Re-executes on every render       | Executes once (setup only)        |

## Stores

`createStore` provides reactive nested objects using proxies. Reads are tracked at the property-path level, so updating `store.user.name` only re-runs effects that read `store.user.name`, not those reading `store.user.email`. Immutable update syntax is enforced via `produce` (Immer-like) or path-based setters.

## Context and Dependency Injection

SolidJS Context (`createContext` / `useContext`) propagates values down the component tree without prop drilling. Unlike React, context consumers don't re-render the whole component — only the specific DOM nodes that read the context value are updated.

## Example

```tsx
import { For, createEffect, createMemo, createSignal } from "solid-js";

export function SearchPreview() {
  const [query, setQuery] = createSignal("");
  const [items] = createSignal(["signal", "memo", "effect", "store"]);

  const matches = createMemo(() =>
    items().filter((item) => item.toLowerCase().includes(query().trim().toLowerCase()))
  );

  createEffect(() => {
    console.log("visible matches", matches().length);
  });

  return (
    <section>
      <input value={query()} onInput={(event) => setQuery(event.currentTarget.value)} />
      <ul>
        <For each={matches()}>{(item) => <li>{item}</li>}</For>
      </ul>
    </section>
  );
}
```
