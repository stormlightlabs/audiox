import { invoke } from "@tauri-apps/api/core";
import { createSignal } from "solid-js";

function App() {
  const [greetMsg, setGreetMsg] = createSignal("");
  const [name, setName] = createSignal("");

  async function greet() {
    setGreetMsg(await invoke("greet", { name: name() }));
  }

  return (
    <main class="container">
      <h1>Hello World</h1>

      <form
        class="row"
        onSubmit={(e) => {
          e.preventDefault();
          greet();
        }}>
        <input id="greet-input" onChange={(e) => setName(e.currentTarget.value)} placeholder="Enter a name..." />
        <button type="submit">Greet</button>
      </form>
      <p>{greetMsg()}</p>
    </main>
  );
}

export default App;
