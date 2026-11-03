// Tiny event hub: network data (weather, holidays) arriving after a screen was drawn.
const listeners = new Set<() => void>();
export const onContextUpdate = (fn: () => void): (() => void) => {
  listeners.add(fn);
  return () => void listeners.delete(fn);
};
export const emitContextUpdate = () => listeners.forEach((l) => l());
