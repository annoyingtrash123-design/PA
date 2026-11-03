// Small DOM helpers and shared widgets (modal sheet, toast, confirm, chips, read-aloud button).
import { icon } from "./icons";
import { onSpeechState, pause, resume, speak, speechState, speechSupported, stop } from "./services/speech";

type Child = Node | string | number | null | undefined | false | Child[];
type Attrs = Record<string, unknown>;

/** Create an element: h("div", { class: "x", onclick: fn }, "text", child). */
export function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Attrs | null = null, ...children: Child[]): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs ?? {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2), v as EventListener);
    else if (k === "class") el.className = String(v);
    else if (k === "style" && typeof v === "object") Object.assign(el.style, v);
    else if (k === "value" || k === "checked" || k === "disabled" || k === "selected") (el as unknown as Record<string, unknown>)[k] = v;
    else el.setAttribute(k, v === true ? "" : String(v));
  }
  append(el, children);
  return el;
}

function append(el: Node, children: Child[]) {
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    if (Array.isArray(c)) append(el, c);
    else el.appendChild(typeof c === "object" ? c : document.createTextNode(String(c)));
  }
}

export function clear(el: Element, ...children: Child[]) {
  el.replaceChildren();
  append(el, children);
}

export const $ = <T extends HTMLElement>(sel: string, root: ParentNode = document) => root.querySelector<T>(sel);

// ---------- Basic building blocks ----------

export const card = (cls: string, ...children: Child[]) => h("section", { class: `card ${cls}`.trim() }, ...children);

export function chip(text: string, opts: { icon?: string; tone?: "lav" | "sage" | "raised" | "solid"; onclick?: () => void } = {}) {
  return h("span", { class: `chip chip-${opts.tone ?? "lav"}`, onclick: opts.onclick }, opts.icon ? icon(opts.icon, 15) : null, text);
}

export const dot = (color: string | null | undefined, size = 10) => h("span", { class: "dot", style: { width: `${size}px`, height: `${size}px`, background: color || "var(--line)" } });

export const muted = (text: string, cls = "") => h("p", { class: `muted ${cls}`.trim() }, text);

export function iconBtn(name: string, label: string, onclick: (e: Event) => void, cls = "", size = 22) {
  return h("button", { type: "button", class: `icon-btn ${cls}`.trim(), "aria-label": label, title: label, onclick }, icon(name, size));
}

export function button(label: string, onclick: (e: Event) => void, opts: { kind?: "primary" | "tonal" | "outline" | "text" | "danger" | "sage"; icon?: string; big?: boolean; disabled?: boolean; cls?: string } = {}) {
  return h("button", { type: "button", class: `btn btn-${opts.kind ?? "primary"} ${opts.big ? "btn-big" : ""} ${opts.cls ?? ""}`.trim(), onclick, disabled: opts.disabled }, opts.icon ? icon(opts.icon, opts.big ? 22 : 18) : null, label);
}

/** Segmented control: calls onchange(value). */
export function segmented<T extends string>(options: [T, string][], value: T, onchange: (v: T) => void) {
  const el = h("div", { class: "seg", role: "tablist" });
  for (const [v, label] of options) {
    el.appendChild(h("button", { type: "button", role: "tab", "aria-selected": v === value ? "true" : "false", class: v === value ? "on" : "", onclick: () => onchange(v) }, label));
  }
  return el;
}

export function field(label: string, input: HTMLElement, hint?: string) {
  return h("label", { class: "field" }, h("span", { class: "field-label" }, label), input, hint ? h("span", { class: "field-hint" }, hint) : null);
}

export function textInput(value: string, opts: Attrs = {}) {
  return h("input", { class: "input", type: "text", value, autocomplete: "off", ...opts });
}

export function select(options: [string, string][], value: string | null, opts: Attrs = {}) {
  const el = h("select", { class: "input", ...opts }, ...options.map(([v, l]) => h("option", { value: v, selected: v === value }, l)));
  if (value !== null) el.value = value;
  return el;
}

export function toggle(label: string, checked: boolean, onchange?: (v: boolean) => void) {
  const input = h("input", { type: "checkbox", checked, onchange: () => onchange?.(input.checked) });
  return { el: h("label", { class: "switch" }, input, h("span", { class: "track" }, h("span", { class: "thumb" })), h("span", { class: "switch-label" }, label)), input };
}

export const loading = (text = "") => h("div", { class: "loading" }, h("span", { class: "spinner" }), text ? h("span", { class: "muted" }, text) : null);

// ---------- Toast ----------

let toastTimer = 0;
export function toast(msg: string, ms = 2800) {
  document.querySelector(".toast")?.remove();
  const t = h("div", { class: "toast", role: "status" }, msg);
  document.body.appendChild(t);
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => t.remove(), ms);
}

// ---------- Modal sheet ----------

export interface Modal {
  close: () => void;
  el: HTMLElement;
}

export function openModal(title: string, body: Child, actions: Child, opts: { onclose?: () => void } = {}): Modal {
  const returnFocus = document.activeElement as HTMLElement | null;
  const close = () => {
    overlay.remove();
    if (!document.querySelector(".sheet-overlay")) document.body.classList.remove("modal-open");
    opts.onclose?.();
    returnFocus?.focus?.();
  };
  const overlay = h(
    "div",
    { class: "sheet-overlay", onclick: (e: Event) => e.target === overlay && close() },
    h(
      "div",
      { class: "sheet", role: "dialog", "aria-modal": "true", "aria-label": title },
      h("div", { class: "sheet-head" }, h("h2", null, title), iconBtn("close", "Close", close)),
      h("div", { class: "sheet-body" }, body),
      h("div", { class: "sheet-actions" }, actions),
    ),
  );
  document.body.appendChild(overlay);
  document.body.classList.add("modal-open");
  return { close, el: overlay };
}

export function confirmDialog(title: string, text: string, confirmLabel: string, danger = true): Promise<boolean> {
  return new Promise((resolve) => {
    let answered = false;
    const done = (v: boolean) => {
      if (answered) return;
      answered = true;
      m.close();
      resolve(v);
    };
    const m = openModal(title, h("p", { class: "muted" }, text), [button("Cancel", () => done(false), { kind: "text" }), button(confirmLabel, () => done(true), { kind: danger ? "danger" : "primary" })], { onclose: () => done(false) });
  });
}

// ---------- Read-aloud button + controls ----------

/** Big "Read it to me" button; while speaking shows pause/resume and stop. */
export function readAloud(getText: () => string, getVoice: () => string, label = "Read it to me", disabled = false, big = true): HTMLElement {
  const wrap = h("div", { class: "read-aloud" });
  if (!speechSupported()) {
    wrap.appendChild(h("p", { class: "muted small" }, "Reading aloud isn't available in this browser."));
    return wrap;
  }
  const draw = () => {
    const st = speechState();
    if (st === "idle") {
      clear(wrap, button(label, () => void speak(getText(), getVoice()).catch((e) => toast(String(e.message ?? e))), big ? { big: true, icon: "play", disabled, cls: "btn-wide" } : { kind: "outline", icon: "volume", disabled }));
    } else {
      clear(
        wrap,
        h(
          "div",
          { class: "player" },
          h("button", { type: "button", class: "round primary", "aria-label": st === "playing" ? "Pause" : "Resume", onclick: () => (st === "playing" ? pause() : resume()) }, icon(st === "playing" ? "pause" : "play", 22)),
          h("button", { type: "button", class: "round", "aria-label": "Stop", onclick: stop }, icon("stop", 22)),
          h("span", { class: "muted" }, st === "playing" ? "Playing..." : "Paused"),
        ),
      );
    }
  };
  draw();
  const off = onSpeechState(() => (wrap.isConnected ? draw() : off()));
  return wrap;
}

export function relativeScrollKeep(fn: () => void) {
  const y = window.scrollY;
  fn();
  window.scrollTo(0, y);
}
