// The wasm-pack output (src/core-wasm/pa_wasm.js, built by `npm run build:core`) or the dev mock; both are
// reached through the "pa-wasm" alias set in vite.config.ts.
declare module "pa-wasm" {
  export default function init(): Promise<unknown>;
  export function initSync(...args: unknown[]): unknown;
  export function init_state(today: string): string;
  export function call(state: string, method: string, path: string, body: string, ctx: string): string;
  export function has_stats(text: string): boolean;
}
