import { api, ApiError } from "../core";
import { icon } from "../icons";
import { fetchBlurb } from "../services/leisure-blurb";
import { hasKey } from "../services/cache";
import { button, clear, confirmDialog, field, h, loading, muted, openModal, segmented, textInput, toast } from "../ui";
import { prettyDate } from "../util/dates";
import { finishedItems, KINDS, kindIcon, openItems } from "../util/leisure";
import type { Leisure, LeisureKind, LeisureStatus } from "../types";

let filter: LeisureKind | "all" = "all";
let finishedOpen = false;

/** "For when you have time": a list of things to watch, read, listen to, play or try. */
export function leisureSection(): HTMLElement {
  const root = h("section", { class: "stack leisure" });
  const draw = () => {
    const items = api<Leisure[]>("GET", "/api/leisure");
    const open = openItems(items, filter);
    const done = finishedItems(items).filter((l) => filter === "all" || l.kind === filter);
    const chips = h("div", { class: "chip-row leisure-filters" }, ...([["all", "All"], ...KINDS.map((k) => [k.id, k.label])] as [string, string][]).map(([id, label]) =>
      h("button", { type: "button", class: `chip chip-btn ${filter === id ? "chip-solid" : "chip-raised"}`, "aria-pressed": String(filter === id), onclick: () => { filter = id as typeof filter; draw(); } }, label)));
    clear(
      root,
      h("div", { class: "row", style: { alignItems: "flex-start", marginTop: "8px" } }, h("div", { class: "grow stack", style: { gap: "2px" } }, h("h2", { class: "title-md" }, "For when you have time"), muted("Things to watch, read, listen to, play or try. I'll only mention one now and then, when your day is quiet.")),
        button("Add", () => editLeisure(null, draw), { kind: "tonal", icon: "add" })),
      chips,
      open.length ? h("div", { class: "leisure-list" }, ...open.map((l) => row(l, draw))) : muted(filter === "all" ? "Nothing here yet. Add a show, a book, a place to try." : "Nothing in this group yet."),
      done.length ? h("button", { type: "button", class: "collapse-head", "aria-expanded": String(finishedOpen), onclick: () => { finishedOpen = !finishedOpen; draw(); } }, h("span", { style: { color: "var(--sage)", display: "flex" } }, icon("tasks", 18)), h("span", { class: "grow", style: { textAlign: "left" } }, `Finished (${done.length})`), h("span", { style: { color: "var(--muted)", display: "flex" } }, icon(finishedOpen ? "expand-less" : "expand-more", 22))) : null,
      done.length && finishedOpen ? h("div", { class: "leisure-list" }, ...done.map((l) => row(l, draw))) : null,
    );
  };
  draw();
  return root;
}

function row(l: Leisure, redraw: () => void): HTMLElement {
  const finished = l.status === "finished";
  return h("button", { type: "button", class: `leisure-row${finished ? " dim" : ""}`, onclick: () => editLeisure(l, redraw) },
    h("span", { class: "leisure-ic" }, icon(kindIcon(l.kind), 20)),
    h("span", { class: "leisure-main" },
      h("span", { class: "leisure-title" }, l.title, l.status === "current" ? h("span", { class: "badge" }, "Currently") : null),
      l.note ? h("span", { class: "muted leisure-note" }, l.note) : null,
      l.blurb ? h("span", { class: "muted leisure-blurb" }, l.blurb) : null,
      finished && l.finished_on ? h("span", { class: "muted leisure-note" }, `Finished ${prettyDate(l.finished_on)}`) : null,
    ),
  );
}

const STATUSES: [LeisureStatus, string][] = [["someday", "Someday"], ["current", "Currently"], ["finished", "Finished"]];

function editLeisure(item: Leisure | null, onSaved: () => void) {
  let kind: LeisureKind = item?.kind ?? (filter === "all" ? "watch" : filter);
  let status: LeisureStatus = item?.status ?? "someday";
  const kindHolder = h("div", { class: "row", style: { justifyContent: "flex-start" } });
  const drawKind = () => clear(kindHolder, segmented(KINDS.map((k) => [k.id, k.label] as [LeisureKind, string]), kind, (v) => { kind = v; drawKind(); }));
  drawKind();
  const stHolder = h("div", { class: "row", style: { justifyContent: "flex-start" } });
  const drawSt = () => clear(stHolder, segmented(STATUSES, status, (v) => { status = v; drawSt(); }));
  drawSt();
  const title = textInput(item?.title ?? "", { placeholder: "e.g. Slow Horses", autocapitalize: "sentences" });
  const note = textInput(item?.note ?? "", { placeholder: "Optional, e.g. episode 3 or who recommended it" });
  const err = h("p", { class: "error-text", hidden: true });
  const fail = (m: string) => { err.textContent = m; err.hidden = false; };
  let blurb = item?.blurb ?? "";
  const blurbBox = h("div", { class: "stack", style: { gap: "6px" } });
  let current = item;
  const drawBlurb = (busy = false) => {
    if (!current) return clear(blurbBox);
    const id = current.id;
    clear(blurbBox,
      h("span", { class: "field-label" }, "About this"),
      busy ? loading("Writing a short description…") : h("p", { class: blurb ? "" : "muted" }, blurb || "No description yet."),
      hasKey() && !busy ? button(blurb ? "Refresh" : "Write a short description", async () => {
        drawBlurb(true);
        const r = await fetchBlurb(id);
        if (r.ok) { blurb = r.item.blurb; current = r.item; } else toast(r.message);
        drawBlurb();
      }, { kind: "text", icon: "insights", cls: "small" }) : null);
  };
  drawBlurb();

  const save = async () => {
    const t = title.value.trim();
    if (!t) return fail("Give it a title first.");
    const body = { kind, title: t, note: note.value.trim(), status, finished_on: status === "finished" ? (item?.finished_on ?? null) : null };
    let saved: Leisure;
    try {
      saved = item ? api<Leisure>("PUT", `/api/leisure/${item.id}`, body) : api<Leisure>("POST", "/api/leisure", body);
    } catch (e) {
      return fail(e instanceof ApiError ? e.message : String(e));
    }
    m.close();
    toast(item ? "Saved" : "Added");
    onSaved();
    // a new item gets ONE short description (only with a key and budget); never for many items at once
    if (!item && hasKey()) {
      const r = await fetchBlurb(saved.id);
      if (r.ok) onSaved();
    }
  };

  const extra = item ? [button("Delete", async () => {
    if (!(await confirmDialog("Delete this?", "It will be removed from your list.", "Delete"))) return;
    try { api("DELETE", `/api/leisure/${item.id}`); } catch (e) { return fail((e as Error).message); }
    m.close();
    toast("Deleted");
    onSaved();
  }, { kind: "text", icon: "delete", cls: "danger" })] : [];
  const body = [h("div", { class: "stack", style: { gap: "6px" } }, h("span", { class: "field-label" }, "Kind"), kindHolder), field("Title", title), field("Note", note), h("div", { class: "stack", style: { gap: "6px" } }, h("span", { class: "field-label" }, "Status"), stHolder), blurbBox, err, extra.length ? h("div", { class: "row wrap", style: { gap: "8px" } }, ...extra) : null];
  const m = openModal(item ? "Edit" : "Add something", body, [button("Cancel", () => m.close(), { kind: "text" }), button("Save", () => void save())]);
  if (!item) title.focus();
}
