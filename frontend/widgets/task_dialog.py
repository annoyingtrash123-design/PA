"""Add / edit task dialog."""
import re

import flet as ft

import theme as T
from api import ApiError
from widgets.common import btn_style, muted

REPEAT = [("none", "Doesn't repeat"), ("daily", "Every day"), ("weekdays", "Weekdays"), ("weekly", "Weekly on chosen days")]
TIME_RE = re.compile(r"^([01]?\d|2[0-3]):([0-5]\d)$")


def derive_part(hhmm):
    h = int(hhmm[:2])
    return "morning" if h < 12 else "afternoon" if h < 17 else "evening"


class TaskDialog:
    def __init__(self, ctx, date_str, occ=None, on_saved=None):
        self.ctx, self.date_str, self.occ, self.on_saved = ctx, date_str, occ, on_saved
        self.task = None  # full task when editing
        self.unknown_recurrence = False
        if occ:
            self.task = self._load_task(occ)
        t = self.task or {"title": "", "time": None, "part_of_day": "anytime", "category_id": None, "notes": "",
                          "recurrence": None, "checklist": []}
        self.t = t
        rec = t.get("recurrence")
        self.title = ft.TextField(label="What needs doing?", value=t["title"], autofocus=not occ, border_radius=14,
                                  capitalization=ft.TextCapitalization.SENTENCES)
        self.use_time = ft.Switch(label="Set a time", value=bool(t.get("time")), active_color=T.LAV, on_change=self._time_toggle)
        self.time = ft.TextField(label="Time (HH:MM)", value=t.get("time") or "09:00", width=150, border_radius=14,
                                 visible=bool(t.get("time")), keyboard_type=ft.KeyboardType.DATETIME)
        self.clock = ft.IconButton(ft.Icons.ACCESS_TIME, visible=bool(t.get("time")), on_click=self._pick_time, icon_color=T.LAV)
        self.part = ft.Dropdown(label="Part of day", value=t.get("part_of_day") or "anytime", border_radius=14,
                                options=[ft.dropdown.Option(k, v) for k, v in T.PART_LABELS.items()], visible=not t.get("time"))
        self.category = ft.Dropdown(label="Category", border_radius=14, width=340,
                                    value=self._category_value(t, bool(occ)),
                                    options=[ft.dropdown.Option("auto", "Auto (let the assistant pick)")] +
                                            [ft.dropdown.Option(str(c["id"]), c["name"]) for c in ctx.categories])
        self.notes = ft.TextField(label="Notes", value=t.get("notes") or "", multiline=True, min_lines=1, max_lines=3, border_radius=14)
        self.repeat = ft.Dropdown(label="Repeat", border_radius=14, on_change=self._repeat_change,
                                  value=rec["kind"] if rec else "none",
                                  options=[ft.dropdown.Option(k, v) for k, v in REPEAT])
        self.days = set(rec.get("days", [])) if rec and rec["kind"] == "weekly" else set()
        self.day_chips = [ft.Chip(label=ft.Text(n), selected=i in self.days, check_color=T.LAV,
                                  selected_color=T.LAV_SOFT, on_select=lambda e, i=i: self._day_sel(i, e.control.selected))
                          for i, n in enumerate(T.WEEKDAYS)]
        self.day_row = ft.Row(self.day_chips, wrap=True, spacing=6, run_spacing=0, visible=bool(rec and rec["kind"] == "weekly"))
        self.items = [{"id": c.get("id"), "field": self._item_field(c["text"])} for c in t.get("checklist", [])]
        self.items_col = ft.Column(spacing=4)
        self._rebuild_items()
        self.error = ft.Text("", color=T.ERROR, size=13, visible=False)

        notes = []
        if occ and occ.get("recurring"):
            notes.append(ft.Container(ft.Text("This is a repeating task: changes apply to the whole series.", size=12.5, color=T.TEXT),
                                      bgcolor=T.LAV_SOFT, padding=10, border_radius=12))
        if self.unknown_recurrence:
            notes.append(muted("Couldn't load this task's repeat settings, so check the Repeat option before saving.", 12))
        content = ft.Column(
            notes + [self.title, self.use_time, ft.Row([self.time, self.clock], spacing=4), self.part, self.category, self.notes,
                     self.repeat, self.day_row, muted("Checklist (optional)", 13), self.items_col,
                     ft.TextButton("Add checklist item", icon=ft.Icons.ADD, on_click=self._add_item),
                     self.error],
            spacing=10, tight=True)
        actions = []
        if occ:
            actions.append(ft.TextButton("Delete", icon=ft.Icons.DELETE_OUTLINE, on_click=self._delete,
                                         style=ft.ButtonStyle(color=T.ERROR)))
        actions += [ft.TextButton("Cancel", on_click=lambda e: self.close(), style=ft.ButtonStyle(color=T.MUTED)),
                    ft.FilledButton("Save", on_click=self._save, style=btn_style())]
        self.dlg = ft.AlertDialog(
            modal=True, bgcolor=T.RAISED, shape=ft.RoundedRectangleBorder(radius=24),
            title=ft.Text("Edit task" if occ else "New task", weight=ft.FontWeight.W_600, color=T.TEXT),
            content=ft.Container(content, width=340), scrollable=True, actions=actions,
            actions_alignment=ft.MainAxisAlignment.END, inset_padding=ft.padding.symmetric(horizontal=12, vertical=16))

    def open(self):
        self.ctx.open(self.dlg)

    def close(self):
        self.ctx.close(self.dlg)

    @staticmethod
    def _category_value(t, editing):
        if not editing or t.get("category_auto"):
            return "auto"
        return str(t["category_id"]) if t.get("category_id") else None

    # --- loading
    def _load_task(self, occ):
        try:
            return self.ctx.api.get_task(occ["task_id"])
        except ApiError:
            # contract has no GET /api/tasks/{id}; rebuild from the occurrence
            self.unknown_recurrence = bool(occ.get("recurring"))
            return {"id": occ["task_id"], "title": occ["title"], "date": None if occ.get("recurring") else occ["date"],
                    "time": occ.get("time"), "part_of_day": occ.get("part_of_day"),
                    "category_id": (occ.get("category") or {}).get("id"), "notes": occ.get("notes") or "",
                    "recurrence": {"kind": "daily"} if occ.get("recurring") else None,
                    "checklist": [{"id": c["id"], "text": c["text"]} for c in occ.get("checklist", [])]}

    # --- handlers
    def _time_toggle(self, e):
        on = self.use_time.value
        self.time.visible = on
        self.clock.visible = on
        self.part.visible = not on
        for c in (self.time, self.clock, self.part):
            c.update()

    def _pick_time(self, e):
        m = TIME_RE.match(self.time.value or "")
        import datetime as dt
        init = dt.time(int(m.group(1)), int(m.group(2))) if m else dt.time(9, 0)

        def chosen(ev):
            v = ev.control.value
            self.time.value = f"{v.hour:02d}:{v.minute:02d}"
            self.time.update()
        self.ctx.page.open(ft.TimePicker(value=init, on_change=chosen))

    def _repeat_change(self, e):
        self.day_row.visible = self.repeat.value == "weekly"
        self.day_row.update()

    def _day_sel(self, i, sel):
        (self.days.add if sel else self.days.discard)(i)

    def _item_field(self, text=""):
        return ft.TextField(value=text, dense=True, border_radius=12, expand=True, hint_text="Step", content_padding=10)

    def _rebuild_items(self):
        rows = []
        for it in self.items:
            rows.append(ft.Row([it["field"], ft.IconButton(ft.Icons.CLOSE, icon_size=18, icon_color=T.MUTED,
                                                          on_click=lambda e, it=it: self._remove_item(it))], spacing=2))
        self.items_col.controls = rows

    def _add_item(self, e):
        self.items.append({"id": None, "field": self._item_field()})
        self._rebuild_items()
        self.items_col.update()

    def _remove_item(self, it):
        self.items.remove(it)
        self._rebuild_items()
        self.items_col.update()

    def _fail(self, msg):
        self.error.value = msg
        self.error.visible = True
        self.error.update()

    def _save(self, e):
        title = (self.title.value or "").strip()
        if not title:
            return self._fail("Give it a title first.")
        time = None
        if self.use_time.value:
            m = TIME_RE.match((self.time.value or "").strip())
            if not m:
                return self._fail("Time should look like 07:30.")
            time = f"{int(m.group(1)):02d}:{m.group(2)}"
        kind = self.repeat.value
        rec = None
        if kind in ("daily", "weekdays"):
            rec = {"kind": kind}
        elif kind == "weekly":
            if not self.days:
                return self._fail("Pick at least one day.")
            rec = {"kind": "weekly", "days": sorted(self.days)}
        if self.occ and self.task and not rec and not self.task.get("recurrence"):
            the_date = self.task.get("date") or self.date_str
        else:
            the_date = None if rec else self.date_str
        if self.occ and not rec and self.task and self.task.get("date"):
            the_date = self.task["date"]
        task = {
            "title": title, "date": the_date, "time": time,
            "part_of_day": derive_part(time) if time else (self.part.value or "anytime"),
            "category_id": (None if self.category.value in (None, "none") else
                            "auto" if self.category.value == "auto" else int(self.category.value)),
            "notes": self.notes.value or "", "recurrence": rec,
            "checklist": [({"id": it["id"], "text": it["field"].value.strip()} if it["id"] else {"text": it["field"].value.strip()})
                          for it in self.items if (it["field"].value or "").strip()],
        }
        try:
            if self.occ:
                saved = self.ctx.api.update_task(self.occ["task_id"], task)
            else:
                saved = self.ctx.api.add_task(task)
        except ApiError as err:
            return self._fail(err.message)
        self.close()
        msg = "Saved"
        if task["category_id"] == "auto" and isinstance(saved, dict):
            name = next((c["name"] for c in self.ctx.categories if c["id"] == saved.get("category_id")), None)
            if name:
                msg = f"Filed under {name}"
        self.ctx.snack(msg)
        if self.on_saved:
            self.on_saved()

    def _delete(self, e):
        try:
            self.ctx.api.delete_task(self.occ["task_id"])
        except ApiError as err:
            return self._fail(err.message)
        self.close()
        self.ctx.snack("Task deleted")
        if self.on_saved:
            self.on_saved()
