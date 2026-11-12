import { useState } from "react";

import { greet, nativeAvailable } from "./lib/native";

/**
 * The starting screen. Replace it.
 *
 * It exists to prove the whole chain works on a new machine: React renders,
 * the button calls a Rust command, and Rust answers from the core crate.
 */
export function App() {
  const [name, setName] = useState("");
  const [reply, setReply] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const ask = async () => {
    setError(null);
    try {
      setReply(await greet(name));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  return (
    <main className="app">
      <h1>Camu</h1>
      <p className="muted">
        {nativeAvailable()
          ? "Running inside the native window."
          : "Running in a browser tab. Start the app with npm run dev to reach Rust."}
      </p>
      <form
        className="row"
        onSubmit={(event) => {
          event.preventDefault();
          void ask();
        }}
      >
        <label className="visually-hidden" htmlFor="name">
          Your name
        </label>
        <input
          id="name"
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder="Your name"
          autoComplete="off"
        />
        <button type="submit">Say hello</button>
      </form>
      {reply && <p>{reply}</p>}
      {error && <p className="error">{error}</p>}
    </main>
  );
}
