"""Goals tab: medium / long-term goals the assistant uses as context."""
from datetime import date

import flet as ft

import theme as T
from widgets.base import BaseView
from widgets.common import btn_style, card, chip, heading, muted

HORIZONS = [("medium", "Medium-term"), ("long", "Long-term")]


def pretty(d):
    return f"{d.day} {d:%b %Y}"


def relative(d, today=None):
    days = (d - (today or date.today())).days
    if days == 0:
        return "today"
    n, past = abs(days), days < 0
    if n < 14:
        txt = f"{n} day{'s' if n != 1 else ''}"
    elif n < 60:
        txt = f"{round(n / 7)} weeks"
    elif n < 700:
        m = round(n / 30.44)
        txt = f"{m} months"
    else:
        y = round(n / 365.25)
        txt = f"{y} years"
    return f"{txt} ago" if past else f"in {txt}"


def deadline_text(g):
    if not g.get("deadline"):
        return "No deadline"
    d = date.fromisoformat(g["deadline"])
    return f"by {pretty(d)} · {relative(d)}"


class GoalsView(BaseView):
    def __init__(self, ctx):
        super().__init__(ctx)
        self.open_sections = {"achieved": False, "archived": False}

    def render(self):
        goals = self.ctx.api.goals()
        by = lambda st, h=None: [g for g in goals if g["status"] == st and (h is None or g["horizon"] == h)]
        out = [ft.Row([
            ft.Column([heading("Goals", 20), muted("Your assistant uses these to see how your days add up.", 13)],
                      spacing=2, expand=True),
            ft.IconButton(ft.Icons.ADD_CIRCLE, icon_color=T.LAV, icon_size=34, tooltip="Add goal",
                          on_click=lambda e: self.edit(None))],
            vertical_alignment=ft.CrossAxisAlignment.START)]
        if not by("active"):
            out.append(card(ft.Column([muted("No active goals yet."),
                                       ft.TextButton("Add your first goal", icon=ft.Icons.FLAG_OUTLINED,
                                                     on_click=lambda e: self.edit(None),
                                                     style=ft.ButtonStyle(color=T.LAV))], spacing=4)))
        for h, label in HORIZONS:
            items = by("active", h)
            if items:
                out.append(ft.Text(label, size=13, weight=ft.FontWeight.W_600, color=T.LAV))
                out += [self._card(g) for g in items]
        for key, label, icon in (("achieved", "Achieved", ft.Icons.CHECK_CIRCLE), ("archived", "Archived", ft.Icons.INVENTORY_2_OUTLINED)):
            items = by(key)
            if items:
                out.append(self._section(key, label, icon, items))
        return out

    def _card(self, g, dim=False):
        parts = [heading(g["title"], 16)]
        if g["status"] == "achieved":
            when = f"Achieved {pretty(date.fromisoformat(g['achieved_on']))}" if g.get("achieved_on") else "Achieved"
            parts.append(ft.Row([ft.Icon(ft.Icons.CHECK_CIRCLE, size=16, color=T.SAGE), ft.Text(when, size=13, color=T.SAGE)], spacing=6))
        else:
            parts.append(ft.Row([chip(deadline_text(g), ft.Icons.EVENT, bgcolor=T.LAV_SOFT),
                                 chip("Long-term" if g["horizon"] == "long" else "Medium-term", bgcolor=T.RAISED, color=T.MUTED)] if dim else
                                [chip(deadline_text(g), ft.Icons.EVENT, bgcolor=T.LAV_SOFT)], spacing=6, wrap=True))
        if g.get("why"):
            parts.append(muted(g["why"], 13.5))
        return card(ft.Column(parts, spacing=8), on_click=lambda e, g=g: self.edit(g), padding=14)

    def _section(self, key, label, icon, items):
        is_open = self.open_sections[key]

        def toggle(e):
            self.open_sections[key] = not is_open
            self.refresh(soft=True)
        head = ft.Container(ft.Row([
            ft.Icon(icon, size=18, color=T.SAGE if key == "achieved" else T.MUTED),
            ft.Text(f"{label} ({len(items)})", size=14, weight=ft.FontWeight.W_600, color=T.TEXT, expand=True),
            ft.Icon(ft.Icons.EXPAND_LESS if is_open else ft.Icons.EXPAND_MORE, color=T.MUTED)], spacing=8),
            on_click=toggle, padding=ft.padding.symmetric(vertical=6))
        col = [head]
        if is_open:
            col += [self._card(g, dim=True) for g in items]
        return ft.Column(col, spacing=10)

    def edit(self, goal):
        GoalDialog(self.ctx, goal, on_saved=lambda: self.refresh(soft=True)).open()


class GoalDialog:
    def __init__(self, ctx, goal, on_saved):
        self.ctx, self.goal, self.on_saved = ctx, goal, on_saved
        g = goal or {"title": "", "horizon": "medium", "deadline": None, "why": "", "status": "active"}
        self.deadline = date.fromisoformat(g["deadline"]) if g.get("deadline") else None
        self.title = ft.TextField(label="Goal", value=g["title"], autofocus=not goal, border_radius=14,
                                  capitalization=ft.TextCapitalization.SENTENCES)
        self.horizon = ft.SegmentedButton(
            selected={g["horizon"]}, allow_multiple_selection=False, allow_empty_selection=False,
            segments=[ft.Segment(value=k, label=ft.Text(v)) for k, v in HORIZONS])
        self.use_deadline = ft.Switch(label="Set a deadline", value=self.deadline is not None, active_color=T.LAV,
                                      on_change=self._dl_toggle)
        self.dl_btn = ft.OutlinedButton(self._dl_label(), icon=ft.Icons.EVENT, on_click=self._pick,
                                        visible=self.deadline is not None, style=ft.ButtonStyle(color=T.LAV))
        self.why = ft.TextField(label="Why", hint_text="Why does this matter to you?", value=g.get("why") or "",
                                multiline=True, min_lines=3, max_lines=5, border_radius=14,
                                capitalization=ft.TextCapitalization.SENTENCES)
        self.error = ft.Text("", color=T.ERROR, size=13, visible=False)
        col = [self.title, muted("Time frame", 13), self.horizon, self.use_deadline, self.dl_btn, self.why, self.error]
        if goal:
            st = goal["status"]
            btns = []
            if st == "active":
                btns.append(ft.FilledButton("Mark achieved", icon=ft.Icons.CHECK, on_click=lambda e: self._status("achieved"),
                                            style=ft.ButtonStyle(bgcolor=T.SAGE, color=T.ON_ACCENT, shape=ft.StadiumBorder())))
                btns.append(ft.OutlinedButton("Archive", icon=ft.Icons.INVENTORY_2_OUTLINED, on_click=lambda e: self._status("archived"),
                                              style=ft.ButtonStyle(color=T.MUTED, shape=ft.StadiumBorder())))
            else:
                btns.append(ft.OutlinedButton("Back to active", icon=ft.Icons.UNDO, on_click=lambda e: self._status("active"),
                                              style=ft.ButtonStyle(color=T.LAV, shape=ft.StadiumBorder())))
            btns.append(ft.TextButton("Delete", icon=ft.Icons.DELETE_OUTLINE, on_click=self._confirm_delete,
                                      style=ft.ButtonStyle(color=T.ERROR)))
            col.append(ft.Row(btns, wrap=True, spacing=8, run_spacing=4))
        self.dlg = ft.AlertDialog(
            modal=True, bgcolor=T.RAISED, shape=ft.RoundedRectangleBorder(radius=24),
            title=ft.Text("Edit goal" if goal else "New goal", weight=ft.FontWeight.W_600, color=T.TEXT),
            content=ft.Container(ft.Column(col, spacing=12, tight=True), width=340), scrollable=True,
            actions=[ft.TextButton("Cancel", on_click=lambda e: self.close(), style=ft.ButtonStyle(color=T.MUTED)),
                     ft.FilledButton("Save", on_click=self._save, style=btn_style())],
            actions_alignment=ft.MainAxisAlignment.END, inset_padding=ft.padding.symmetric(horizontal=12, vertical=16))

    def open(self):
        self.ctx.open(self.dlg)

    def close(self):
        self.ctx.close(self.dlg)

    def _dl_label(self):
        return pretty(self.deadline) if self.deadline else "Pick a date"

    def _dl_toggle(self, e):
        self.dl_btn.visible = self.use_deadline.value
        if self.use_deadline.value and not self.deadline:
            self._pick(None)
        self.dl_btn.update()

    def _pick(self, e):
        def chosen(ev):
            v = ev.control.value
            self.deadline = v.date() if hasattr(v, "date") else v
            self.dl_btn.text = self._dl_label()
            self.dl_btn.update()
        self.ctx.page.open(ft.DatePicker(value=self.deadline, first_date=date(2020, 1, 1), last_date=date(2100, 12, 31),
                                         on_change=chosen))

    def _fail(self, msg):
        self.error.value = msg
        self.error.visible = True
        self.error.update()

    def _body(self, status=None):
        title = (self.title.value or "").strip()
        if not title:
            self._fail("Give your goal a title first.")
            return None
        if self.use_deadline.value and not self.deadline:
            self._fail("Pick a deadline date, or switch it off.")
            return None
        g = self.goal or {}
        st = status or g.get("status", "active")
        return {"title": title, "horizon": next(iter(self.horizon.selected), "medium"),
                "deadline": self.deadline.isoformat() if self.use_deadline.value else None,
                "why": (self.why.value or "").strip(), "status": st,
                "achieved_on": (date.today().isoformat() if st == "achieved" and g.get("status") != "achieved" else g.get("achieved_on"))
                if st == "achieved" else None}

    def _send(self, body, msg):
        from api import ApiError
        try:
            if self.goal:
                self.ctx.api.update_goal(self.goal["id"], body)
            else:
                self.ctx.api.add_goal(body)
        except ApiError as err:
            return self._fail(err.message)
        self.close()
        self.ctx.snack(msg)
        self.on_saved()

    def _save(self, e):
        body = self._body()
        if body:
            self._send(body, "Goal saved")

    def _status(self, status):
        body = self._body(status)
        if body:
            self._send(body, {"achieved": "Goal achieved, well done!", "archived": "Goal archived", "active": "Goal is active again"}[status])

    def _confirm_delete(self, e):
        def yes(ev):
            from api import ApiError
            try:
                self.ctx.api.delete_goal(self.goal["id"])
            except ApiError as err:
                self.ctx.close(conf)
                return self._fail(err.message)
            self.ctx.close(conf)
            self.close()
            self.ctx.snack("Goal deleted")
            self.on_saved()
        conf = ft.AlertDialog(
            modal=True, bgcolor=T.RAISED, shape=ft.RoundedRectangleBorder(radius=24),
            title=ft.Text("Delete this goal?", color=T.TEXT, weight=ft.FontWeight.W_600),
            content=ft.Text("This can't be undone. Archive it instead if you just want it out of the way.", color=T.MUTED),
            actions=[ft.TextButton("Keep it", on_click=lambda ev: self.ctx.close(conf), style=ft.ButtonStyle(color=T.MUTED)),
                     ft.FilledButton("Delete", on_click=yes, style=ft.ButtonStyle(bgcolor=T.ERROR, color=T.ON_ACCENT,
                                                                                  shape=ft.RoundedRectangleBorder(radius=24)))])
        self.ctx.open(conf)
