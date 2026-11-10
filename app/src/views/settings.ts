import { api, ApiError, assistantName, getState, resetEverything, today } from "../core";
import { aiUsage, testKey } from "../services/ai-text";
import { cleanKey, DEFAULT_MODEL, getKey, getModel, hasKey, setKey, setModel } from "../services/cache";
import { refreshHolidays } from "../services/holidays";
import { kvClear } from "../services/idb";
import { listVoices } from "../services/speech";
import { resetWeather } from "../services/weather";
import { button, card, clear, confirmDialog, field, h, iconBtn, muted, openModal, readAloud, segmented, select, textInput, toast } from "../ui";
import { stopAll, previewNatural, refreshSpeed } from "../services/readaloud";
import { STYLES, VOICES } from "../services/gemini-tts";
import { icon } from "../icons";
import { DEFAULT_SPEED, getGeminiVoice, getFavourites, getSpeed, getStyle, getVoiceType, orderVoices, toggleFavourite, SPEED_MAX, SPEED_MIN, SPEED_STEP, savePrefs, type VoiceType } from "../services/voice-prefs";
import { addExtra, quotaMessage } from "../services/ai-budget";
import { GeminiError } from "../services/gemini";
import { decodeProfile, ProfileCodeError, summarize, type ApplySummary } from "../util/profile-code";
import { isValidTimezone } from "../util/dates";
import type { Category, Memory } from "../types";

const PALETTE = ["#EC6A5C", "#F59E0B", "#E8B931", "#6BAA75", "#4CAF50", "#26A69A", "#4FA3D1", "#5C6BC0", "#9575CD", "#D5679B", "#A1887F", "#78909C"];
const HHMM = /^([01]\d|2[0-3]):[0-5]\d$/;
const sub = (t: string) => h("p", { class: "label-caps" }, t);
const outlineBtn = (label: string, fn: () => void, ic = "check") => button(label, fn, { kind: "outline", icon: ic });

const OPEN_KEY = "pa-settings-open";
function openSections(): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(OPEN_KEY) || "[]"));
  } catch {
    return new Set();
  }
}

/** Turn every settings card that starts with an h3 into a collapsible section (closed by default; remembers what's open). */
function makeCollapsible(root: HTMLElement): void {
  const open = openSections();
  root.querySelectorAll<HTMLElement>("section.card").forEach((c) => {
    const head = c.firstElementChild;
    if (!(head instanceof HTMLHeadingElement) || c.dataset.collapsible) return;
    c.dataset.collapsible = "1";
    const key = (head.textContent || "").replace(/^What .* knows about you$/, "What knows about you");
    const body = h("div", { class: "stack collapse-body" });
    while (head.nextSibling) body.appendChild(head.nextSibling);
    const btn = h("button", { type: "button", class: "collapse-head", "aria-expanded": "false" }, h("span", { class: "title-sm" }, head.textContent || ""), icon("right", 22));
    head.replaceWith(btn);
    c.appendChild(body);
    const set = (isOpen: boolean) => {
      body.hidden = !isOpen;
      btn.setAttribute("aria-expanded", String(isOpen));
      c.classList.toggle("open", isOpen);
    };
    set(open.has(key));
    btn.addEventListener("click", () => {
      const now = !c.classList.contains("open");
      set(now);
      const s = openSections();
      if (now) s.add(key);
      else s.delete(key);
      try {
        localStorage.setItem(OPEN_KEY, JSON.stringify([...s]));
      } catch {
        // per-viewer convenience only
      }
    });
  });
}

export function renderSettings(root: HTMLElement): void {
  const draw = () => {
    const y = window.scrollY;
    clear(root, ...build(draw));
    makeCollapsible(root);
    window.scrollTo(0, y);
  };
  draw();
}

function build(redraw: () => void): HTMLElement[] {
  const s = getState().settings;
  const name = assistantName();

  // ---- about you
  const fAssistant = textInput(s.assistant_name ?? "");
  const fUser = textInput(s.user_name ?? "", { placeholder: "What should I call you?" });

  // ---- voice
  const fVoice = select([["", "Automatic (best available)"]], "");
  void listVoices().then((vs) => {
    const sorted = [...vs].sort((a, b) => a.name.localeCompare(b.name));
    for (const v of sorted) fVoice.appendChild(h("option", { value: v.name }, `${v.name} (${v.lang})`));
    fVoice.value = s.voice && sorted.some((v) => v.name === s.voice) ? s.voice : "";
    if (!sorted.length) fVoice.appendChild(h("option", { value: "", disabled: true }, "No English voices found on this device"));
  });
  // ---- location
  const fLoc = textInput(s.location_name ?? "");
  const fLat = h("input", { class: "input", type: "text", inputmode: "decimal", value: String(s.latitude ?? "") });
  const fLon = h("input", { class: "input", type: "text", inputmode: "decimal", value: String(s.longitude ?? "") });
  const fTz = textInput(s.timezone ?? "", { placeholder: "e.g. Australia/Sydney" });
  const geoStatus = h("p", { class: "muted small", hidden: true });
  const useLocation = () => {
    if (!("geolocation" in navigator)) return toast("Location isn't available in this browser");
    geoStatus.hidden = false;
    geoStatus.textContent = "Finding you…";
    navigator.geolocation.getCurrentPosition(
      (p) => {
        fLat.value = p.coords.latitude.toFixed(4);
        fLon.value = p.coords.longitude.toFixed(4);
        geoStatus.textContent = "Got it. Name the place above, then save.";
      },
      (e) => (geoStatus.textContent = e.code === 1 ? "Location permission was declined." : "Couldn't get your location."),
      { timeout: 10000, maximumAge: 600000 },
    );
  };

  // ---- gemini
  const fKey = h("input", { class: "input", type: "password", autocomplete: "off", autocapitalize: "off", autocorrect: "off", spellcheck: "false", placeholder: hasKey() ? "Key saved (leave empty to keep it)" : "Paste your key", "aria-label": "Gemini API key" });
  const fModel = textInput(getModel(), { placeholder: DEFAULT_MODEL });
  const keyStatus = h("p", { class: "small", hidden: true });
  const usageText = () => {
    const u = aiUsage();
    return `AI writing today: ${u.used} of ${u.limit} · Voice clips today: ${u.voiceUsed} of ${u.voiceLimit} (a summary is read in about 3 clips)`;
  };
  const usageLine = h("p", { class: "muted small" }, usageText());
  // the app's own daily limit is a safety net below Google's; this lets a few more through today when it's run out
  const extraBtn = button("Allow 3 more today", () => {
    addExtra(today(), 3);
    usageLine.textContent = usageText();
    toast("Three more for today. If Google's own free limit is reached, Miranda will wait until it resets.");
  }, { kind: "text", cls: "small", icon: "add" });
  const runTest = async () => {
    const key = cleanKey(fKey.value) || getKey();
    if (!key) return setStatus("Paste a key first.", true);
    setStatus("Testing…", false);
    try {
      await testKey(key, fModel.value.trim() || DEFAULT_MODEL);
      setStatus("Key works.", false, true);
      usageLine.textContent = usageText();
    } catch (e) {
      if (e instanceof GeminiError && e.quota) setStatus(quotaMessage(e.pausedUntil ?? Date.now() + 3600_000, Date.now()), true);
      else setStatus(`That didn't work: ${(e as Error).message}`, true);
      usageLine.textContent = usageText();
    }
  };
  const setStatus = (t: string, bad: boolean, ok = false) => {
    keyStatus.hidden = false;
    keyStatus.textContent = t;
    keyStatus.className = `small ${bad ? "error-text" : ok ? "status-ok" : "muted"}`;
  };

  const save = async () => {
    const lat = fLat.value.trim() === "" ? undefined : Number(fLat.value.replace(",", "."));
    const lon = fLon.value.trim() === "" ? undefined : Number(fLon.value.replace(",", "."));
    if ((lat !== undefined && !Number.isFinite(lat)) || (lon !== undefined && !Number.isFinite(lon))) return toast("Latitude and longitude should be numbers");
    if (lat !== undefined && Math.abs(lat) > 90) return toast("Latitude should be between -90 and 90");
    if (lon !== undefined && Math.abs(lon) > 180) return toast("Longitude should be between -180 and 180");
    const tz = fTz.value.trim() || "Australia/Sydney";
    if (!isValidTimezone(tz)) return toast("That timezone isn't recognised. Try something like Australia/Sydney.");
    const before = getState().settings;
    const body: Record<string, unknown> = { assistant_name: fAssistant.value.trim() || "Miranda", user_name: fUser.value.trim(), voice: fVoice.value, location_name: fLoc.value.trim(), timezone: tz };
    if (lat !== undefined) body.latitude = lat;
    if (lon !== undefined) body.longitude = lon;
    try {
      api("PUT", "/api/settings", body);
    } catch (e) {
      return toast(e instanceof ApiError ? e.message : String(e));
    }
    if (fKey.value.trim()) setKey(fKey.value);
    setModel(fModel.value.trim() === DEFAULT_MODEL ? "" : fModel.value);
    const after = getState().settings;
    if (before.latitude !== after.latitude || before.longitude !== after.longitude) void resetWeather();
    window.dispatchEvent(new Event("pa-settings"));
    toast("Saved. Looking good!");
    redraw();
  };

  // ---- backup / data
  const download = () => {
    const data = api("GET", "/api/export");
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
    const a = h("a", { href: URL.createObjectURL(blob), download: `pa-backup-${new Date().toISOString().slice(0, 10)}.json` });
    document.body.appendChild(a);
    a.click();
    setTimeout(() => { URL.revokeObjectURL(a.href); a.remove(); }, 1000);
  };
  const filePick = h("input", { type: "file", accept: "application/json,.json", hidden: true });
  filePick.onchange = async () => {
    const f = filePick.files?.[0];
    filePick.value = "";
    if (!f) return;
    let parsed: unknown;
    try {
      parsed = JSON.parse(await f.text());
    } catch {
      return toast("That file isn't valid JSON");
    }
    if (!(await confirmDialog("Restore from backup?", "This replaces everything in the app (tasks, goals, gym, settings) with the contents of the file.", "Restore"))) return;
    try {
      api("POST", "/api/import", parsed);
    } catch (e) {
      return toast(e instanceof ApiError ? e.message : String(e));
    }
    window.dispatchEvent(new Event("pa-settings"));
    void resetWeather();
    void refreshHolidays();
    toast("Backup restored");
    redraw();
  };
  const loadSample = async () => {
    if (!(await confirmDialog("Load sample data?", "This adds demo tasks, goals and workouts so you can look around. Your own data stays.", "Load it", false))) return;
    try {
      api("POST", "/api/sample-data");
    } catch (e) {
      return toast((e as Error).message);
    }
    toast("Sample data loaded");
    redraw();
  };
  const reset = async () => {
    if (!(await confirmDialog("Reset everything?", "This deletes all your tasks, goals, workouts and settings from this device.", "Continue"))) return;
    if (!(await confirmDialog("Really reset everything?", "Last chance: this can't be undone. Download a backup first if you might want it back.", "Yes, delete it all"))) return;
    await resetEverything();
    await kvClear("pa-ai-cache");
    window.dispatchEvent(new Event("pa-settings"));
    toast("Everything was reset");
    redraw();
  };

  return [
    h("h2", { class: "title-lg", style: { fontSize: "22px" } }, "Settings"),
    card("stack", h("h3", { class: "title-sm" }, "About you"), field("Assistant name", fAssistant), field("Your name", fUser)),
    voiceCard(fVoice, fAssistant),
    card("stack", h("h3", { class: "title-sm" }, "Weather location"), field("Location name", fLoc), h("div", { class: "pair" }, field("Latitude", fLat), field("Longitude", fLon)), button("Use my location", useLocation, { kind: "outline", icon: "place" }), geoStatus, field("Timezone", fTz, "Decides what \"today\" means, e.g. Australia/Sydney")),
    card("stack", h("h3", { class: "title-sm" }, "AI (Gemini)"),
      muted(`Optional. With a key, ${name} writes chattier briefings. Without one you still get friendly template texts. The key stays on this device and is never included in backups.`),
      field("Gemini API key", fKey, hasKey() ? `A key is saved, ending in …${getKey().slice(-4)}. Match it on aistudio.google.com/apikey to see which Google account it belongs to.` : undefined),
      h("a", { class: "link small", href: "https://aistudio.google.com/apikey", target: "_blank", rel: "noopener noreferrer" }, "Get a free key at Google AI Studio"),
      field("Model", fModel, `Default ${DEFAULT_MODEL}`), h("div", { class: "row wrap" }, outlineBtn("Test key", () => void runTest(), "check"), hasKey() ? button("Remove key", () => { setKey(""); toast("Key removed"); redraw(); }, { kind: "text", cls: "danger" }) : null), keyStatus, usageLine, extraBtn, h("p", { class: "muted small" }, "Miranda writes each summary once a day; tap 'update summary' if your plans change.")),
    button("Save settings", () => void save(), { big: true, icon: "check", cls: "btn-wide" }),
    memoryCard(name),
    profileCard(redraw),
    categoriesCard(),
    card("stack", h("h3", { class: "title-sm" }, "Backup"), muted("Your data lives only on this device. Download a backup now and then."), h("div", { class: "row wrap" }, button("Download backup", download, { kind: "outline", icon: "download" }), button("Restore from backup", () => filePick.click(), { kind: "outline", icon: "upload" })), filePick),
    card("stack", h("h3", { class: "title-sm" }, "Data"), h("div", { class: "row wrap" }, button("Load sample data", () => void loadSample(), { kind: "outline", icon: "add" }), button("Reset everything", () => void reset(), { kind: "text", icon: "delete", cls: "danger" }))),
    h("p", { class: "muted small", style: { textAlign: "center" } }, "Miranda runs entirely on your device."),
  ].filter(Boolean) as HTMLElement[];
}

// ---------------- assistant memory ----------------
function memoryCard(name: string): HTMLElement {
  let mem: Memory;
  try {
    mem = api<Memory>("GET", "/api/memory");
  } catch (e) {
    return card("stack", h("h3", { class: "title-sm" }, `What ${name} knows about you`), muted((e as Error).message));
  }
  const rh = mem.rhythm;
  const time = (v: string, label: string) => h("input", { class: "input", type: "time", value: v, "aria-label": label });
  const wf = time(rh.wake_from, "Wake from"), wt = time(rh.wake_to, "Wake to"), sf = time(rh.sleep_from, "Asleep from"), st = time(rh.sleep_to, "Asleep to");
  const saveRhythm = () => {
    const body = { wake_from: wf.value, wake_to: wt.value, sleep_from: sf.value, sleep_to: st.value };
    if (!Object.values(body).every((v) => HHMM.test(v))) return toast("Times should look like 06:30");
    try { api("PUT", "/api/memory/rhythm", body); toast("Daily rhythm saved"); } catch (e) { toast((e as Error).message); }
  };

  return card("stack",
    h("h3", { class: "title-sm" }, `What ${name} knows about you`),
    muted(`${name} uses this to write your daily digest and briefings.`),
    sub("Daily rhythm"), h("span", null, "Usually wake"), h("div", { class: "pair" }, wf, wt), h("span", null, "Usually asleep by"), h("div", { class: "pair" }, sf, st), outlineBtn("Save daily rhythm", saveRhythm),
  );
}

// ---------------- private profile ----------------
function profileCard(redraw: () => void): HTMLElement {
  let updated = "";
  try { updated = api<Memory>("GET", "/api/memory").profile_updated ?? ""; } catch { /* shown as not set */ }
  const when = updated ? new Date(`${updated}T12:00:00`).toLocaleDateString("en-AU", { day: "numeric", month: "short" }) : "";
  const ta = h("textarea", { class: "input", rows: 4, autocomplete: "off", spellcheck: false, "aria-label": "Paste a profile code", placeholder: "MIRANDA2:…" }) as HTMLTextAreaElement;
  const applyText = async (text: string) => {
    try {
      const r = api<ApplySummary>("POST", "/api/profile/apply", await decodeProfile(text));
      ta.value = "";
      toast(summarize(r));
      if (r.work) void refreshHolidays();
      window.dispatchEvent(new Event("pa-settings"));
      redraw();
    } catch (e) {
      toast(e instanceof ProfileCodeError || e instanceof ApiError ? e.message : String(e));
    }
  };
  const apply = () => {
    if (!ta.value.trim()) return toast("Paste a profile code first");
    void applyText(ta.value);
  };
  const fileIn = h("input", { type: "file", accept: ".txt,text/*,*/*", hidden: true }) as HTMLInputElement;
  fileIn.onchange = async () => {
    const f = fileIn.files?.[0];
    fileIn.value = "";
    if (!f) return;
    let text = "";
    try { text = await f.text(); } catch { return toast("Couldn't read that file"); }
    if (!text.trim()) return toast("That file is empty");
    await applyText(text);
  };
  const clearIt = async () => {
    if (!(await confirmDialog("Clear private profile?", "This removes the private interests, notes, work hours and weather preferences. Goals and leisure items it added stay.", "Clear"))) return;
    try { api("POST", "/api/profile/clear", {}); } catch (e) { return toast((e as Error).message); }
    window.dispatchEvent(new Event("pa-settings"));
    toast("Private profile cleared");
    redraw();
  };
  return card("stack",
    h("h3", { class: "title-sm" }, "Private profile & imports"),
    h("p", { class: "small" }, updated ? `Private profile: set · updated ${when}` : "Not set yet"),
    muted("Your interests, notes, work hours and weather preferences are private and aren't shown here. Codes from Claude can also add goals, tasks and reading-list items."),
    field("Paste a profile code", ta),
    h("div", { class: "row wrap" }, outlineBtn("Apply", apply), outlineBtn("Load code from file", () => fileIn.click()), fileIn, updated ? button("Clear private profile", () => void clearIt(), { kind: "text", cls: "danger" }) : null),
  );
}

// ---------------- categories ----------------
function categoriesCard(): HTMLElement {
  const col = h("div", { class: "stack", style: { gap: "6px" } });
  let newColor = PALETTE[0];
  const newName = textInput("", { placeholder: "New category" });
  const newDot = h("button", { type: "button", class: "dotbtn", "aria-label": "Pick colour" });
  const paintNew = () => clear(newDot, h("span", { class: "dot", style: { width: "26px", height: "26px", background: newColor } }));
  paintNew();
  const pick = (current: string, done: (c: string) => void) => {
    const m = openModal("Pick a colour", h("div", { class: "swatches" }, ...PALETTE.map((c) => h("button", { type: "button", class: "swatch", style: { background: c, borderColor: c === current ? "var(--text)" : "transparent" }, "aria-label": c, onclick: () => { m.close(); done(c); } }))), []);
  };
  newDot.onclick = () => pick(newColor, (c) => { newColor = c; paintNew(); });
  const reload = () => clear(col, ...getState().categories.map((c) => row(c)));
  const row = (c: Category) => {
    const input = textInput(c.name, { "aria-label": `Category name ${c.name}` });
    const rename = () => {
      const v = input.value.trim();
      if (!v || v === c.name) return (input.value = c.name);
      try { api("PUT", `/api/categories/${c.id}`, { name: v, color: c.color }); toast("Category renamed"); reload(); } catch (e) { toast((e as Error).message); input.value = c.name; }
    };
    input.onchange = rename;
    const dotBtn = h("button", { type: "button", class: "dotbtn", "aria-label": `Change colour of ${c.name}`, onclick: () => pick(c.color, (col2) => { try { api("PUT", `/api/categories/${c.id}`, { name: c.name, color: col2 }); reload(); } catch (e) { toast((e as Error).message); } }) }, h("span", { class: "dot", style: { width: "26px", height: "26px", background: c.color } }));
    return h("div", { class: "cat-row" }, dotBtn, h("div", { class: "grow" }, input), iconBtn("delete", `Delete ${c.name}`, () => { try { api("DELETE", `/api/categories/${c.id}`); reload(); } catch (e) { toast((e as Error).message); } }, "muted sm", 20));
  };
  reload();
  const add = () => {
    const n = newName.value.trim();
    if (!n) return;
    try { api("POST", "/api/categories", { name: n, color: newColor }); newName.value = ""; reload(); } catch (e) { toast((e as Error).message); }
  };
  newName.onkeydown = (e) => { if (e.key === "Enter") add(); };
  return card("stack", h("h3", { class: "title-sm" }, "Categories"), col, h("div", { class: "cat-row" }, newDot, h("div", { class: "grow" }, newName), iconBtn("add-circle", "Add category", add, "lav", 32)));
}


// ---------------- voice (natural Gemini voice or the phone voice) ----------------
function voiceCard(fVoice: HTMLSelectElement, fAssistant: HTMLInputElement): HTMLElement {
  const type = getVoiceType();
  const style = getStyle();
  let voice = getGeminiVoice();
  const rows = new Map<string, HTMLElement>();
  const fStyle = select(STYLES.map((x) => [x.id, x.label] as [string, string]), style);
  fStyle.addEventListener("change", () => savePrefs({ style: fStyle.value }));

  const speedVal = getSpeed();
  const fSpeed = h("input", { type: "range", min: String(SPEED_MIN), max: String(SPEED_MAX), step: String(SPEED_STEP), value: String(speedVal), "aria-label": "Speaking speed", style: { width: "100%" } }) as HTMLInputElement;
  const speedLabel = h("span", { class: "small" }, "");
  const showSpeed = () => {
    const v = getSpeed();
    speedLabel.textContent = v === DEFAULT_SPEED ? `${v.toFixed(2)}× (my default)` : `${v.toFixed(2)}×`;
  };
  showSpeed();
  fSpeed.addEventListener("input", () => {
    savePrefs({ speed: Number(fSpeed.value) });
    showSpeed();
    refreshSpeed();
  });
  const speedBox = h(
    "div",
    { class: "stack" },
    h("div", { class: "row", style: { justifyContent: "space-between" } }, h("span", { class: "label-caps" }, "Speaking speed"), speedLabel),
    fSpeed,
    h("div", { class: "row small", style: { justifyContent: "space-between" } }, h("span", {}, "Slower"), h("span", {}, "Normal")),
  );

  const previewBtn = (name: string) => {
    const b = h("button", { type: "button", class: "btn btn-outline", "aria-label": `Preview ${name}`, style: { minHeight: "36px", padding: "6px 12px" } }, "▶ Preview");
    b.onclick = async () => {
      b.disabled = true;
      b.textContent = "…";
      try {
        await previewNatural(name, fStyle.value);
      } catch (e) {
        toast(`Couldn't make the preview: ${(e as Error).message}`);
      } finally {
        b.disabled = false;
        b.textContent = "▶ Preview";
      }
    };
    return b;
  };
  const list = h("div", { class: "voice-list", role: "radiogroup", "aria-label": "Natural voice" });
  const drawList = () => {
    rows.clear();
    const favs = getFavourites();
    const { favourites, others } = orderVoices(VOICES, favs);
    const row = (v: (typeof VOICES)[number]) => {
      const fav = favs.includes(v.name);
      const el = h(
        "div",
        { class: `voice-row ${v.name === voice ? "on" : ""}` },
        h("button", { type: "button", class: "pick", role: "radio", "aria-checked": String(v.name === voice), onclick: () => {
          voice = v.name;
          savePrefs({ voice });
          rows.forEach((r, n) => {
            r.classList.toggle("on", n === voice);
            r.querySelector(".pick")?.setAttribute("aria-checked", String(n === voice));
          });
        } }, `${v.name} — ${v.trait}`),
        h("button", { type: "button", class: "btn btn-outline", "aria-pressed": String(fav), "aria-label": fav ? `Remove ${v.name} from favourites` : `Add ${v.name} to favourites`, style: { minHeight: "36px", padding: "6px 10px" }, onclick: () => {
          toggleFavourite(v.name);
          drawList();
        } }, fav ? "★" : "☆"),
        previewBtn(v.name),
      );
      rows.set(v.name, el);
      return el;
    };
    clear(list, ...(favourites.length ? [h("p", { class: "label-caps muted small" }, "Favourites"), ...favourites.map(row), h("p", { class: "label-caps muted small" }, "All voices")] : []), ...others.map(row));
  };
  drawList();
  const naturalBox = h(
    "div",
    { class: "stack", hidden: type !== "gemini" },
    list,
    field("Speaking style", fStyle),
    muted("Uses your free Gemini allowance; each summary's audio is made once and saved.", "small"),
  );
  const phoneBox = h(
    "div",
    { class: "stack", hidden: type !== "phone" },
    field("Voice", fVoice, "Voices come from your device, so the list differs between phones and browsers."),
    readAloud(() => `Hi! I'm ${fAssistant.value.trim() || "Miranda"}. This is how I'll sound when I brief you.`, () => fVoice.value, "Preview voice", false, false, { forcePhone: true }),
  );
  const seg = segmented<VoiceType>([["gemini", "Natural (Gemini)"], ["phone", "Phone voice"]], type, (v) => {
    stopAll();
    savePrefs({ type: v });
    naturalBox.hidden = v !== "gemini";
    phoneBox.hidden = v !== "phone";
    seg.querySelectorAll("button").forEach((b, i) => {
      const on = (i === 0) === (v === "gemini");
      b.className = on ? "on" : "";
      b.setAttribute("aria-selected", String(on));
    });
  });
  return card("stack", h("h3", { class: "title-sm" }, "Voice"), seg, naturalBox, phoneBox, speedBox);
}
