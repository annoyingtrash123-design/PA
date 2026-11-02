from datetime import date, timedelta

import flet as ft

import theme as T
from views.home import pretty_date
from widgets.base import BaseView
from widgets.common import OccurrenceTile, card, chip, heading, muted, now_in
from widgets.task_dialog import TaskDialog


def monday_of(d):
    return d - timedelta(days=d.weekday())


class TasksView(BaseView):
    def __init__(self, ctx):
        super().__init__(ctx)
        self.week = monday_of(date.today())

    def go(self, delta=None, reset=False):
        self.week = monday_of(date.today()) if reset else self.week + timedelta(days=7 * delta)
        self.refresh()

    def render(self):
        days = self.ctx.api.days(self.week.isoformat(), 7)
        end = self.week + timedelta(days=6)
        rng = f"{self.week.day} {self.week:%b} - {end.day} {end:%b}"
        this = self.week == monday_of(date.today())
        header = ft.Row([
            ft.IconButton(ft.Icons.CHEVRON_LEFT, on_click=lambda e: self.go(-1), icon_color=T.TEXT, tooltip="Previous week"),
            ft.Column([heading(rng, 17), muted("This week" if this else f"{self.week.year}", 12)], spacing=0,
                      horizontal_alignment=ft.CrossAxisAlignment.CENTER, expand=True),
            ft.IconButton(ft.Icons.CHEVRON_RIGHT, on_click=lambda e: self.go(1), icon_color=T.TEXT, tooltip="Next week"),
        ], alignment=ft.MainAxisAlignment.SPACE_BETWEEN)
        out = [header]
        if not this:
            out.append(ft.Row([ft.OutlinedButton("This week", icon=ft.Icons.TODAY, on_click=lambda e: self.go(reset=True),
                                                 style=ft.ButtonStyle(color=T.LAV, shape=ft.StadiumBorder()))],
                              alignment=ft.MainAxisAlignment.CENTER))
        for d in days:
            out.append(self._day(d))
        return out

    def _day(self, d):
        dt = date.fromisoformat(d["date"])
        is_today = dt == now_in(self.ctx).date()
        title = ft.Text(f"{d['weekday']} {dt.day} {dt:%b}", size=16, weight=ft.FontWeight.W_700,
                        color=T.LAV if is_today else T.TEXT)
        head = [title]
        if is_today:
            head.append(chip("Today", bgcolor=T.LAV, color=T.ON_ACCENT))
        wk = d.get("workout")
        if wk:
            head.append(chip("Rest" if wk.get("rest") else wk["name"],
                             ft.Icons.SPA_OUTLINED if wk.get("rest") else ft.Icons.FITNESS_CENTER,
                             bgcolor=T.SAGE_SOFT if wk.get("rest") else T.LAV_SOFT))
        if d.get("holiday"):
            head.append(chip(d["holiday"], ft.Icons.CELEBRATION_OUTLINED, bgcolor=T.SAGE_SOFT))
        if d.get("day_off"):
            head.append(chip("Day off", ft.Icons.BEACH_ACCESS_OUTLINED, bgcolor=T.SAGE_SOFT))
        off = bool(d.get("day_off"))
        menu = ft.PopupMenuButton(
            icon=ft.Icons.MORE_VERT, icon_color=T.MUTED, tooltip="Day options",
            items=[ft.PopupMenuItem(text="Undo day off" if off else "Mark as day off",
                                    icon=ft.Icons.EVENT_AVAILABLE if off else ft.Icons.BEACH_ACCESS_OUTLINED,
                                    on_click=lambda e, ds=d["date"], off=off: self._toggle_off(ds, off))])
        top = ft.Row([ft.Row(head, spacing=8, wrap=True, expand=True), menu,
                      ft.IconButton(ft.Icons.ADD_CIRCLE, icon_color=T.LAV, icon_size=30, tooltip="Add task",
                                    on_click=lambda e, ds=d["date"]: self.add(ds))],
                     vertical_alignment=ft.CrossAxisAlignment.CENTER)
        col = [top]
        if not d["items"] and not d.get("work"):
            col.append(muted("Nothing planned", 13))
        work = d.get("work")
        entries = [((o.get("time") or "99:99"), 1, o) for o in d["items"]]
        if work:
            entries.append((work["start"], 0, None))
        entries.sort(key=lambda x: (x[0] if x[0] != "99:99" else "99:99", x[1]))
        for _, _, o in entries:
            if o is None:
                col.append(self._work_band(work))
            else:
                col.append(OccurrenceTile(self.ctx, o, lambda: self.refresh(soft=True), on_open=self.edit))
        return card(ft.Column(col, spacing=4), bgcolor=T.RAISED if is_today else T.CARD,
                    border_color=T.LAV if is_today else T.LINE, padding=14)

    def _work_band(self, w):
        return ft.Container(ft.Row([ft.Icon(ft.Icons.WORK_OUTLINE, size=18, color=T.LAV),
                                    ft.Text(f"Work {w['start']}-{w['end']}", size=14, weight=ft.FontWeight.W_500, color=T.TEXT)],
                                   spacing=10), bgcolor=T.LAV_SOFT, border_radius=12,
                            padding=ft.padding.symmetric(horizontal=12, vertical=9), margin=ft.margin.symmetric(vertical=3))

    def _toggle_off(self, ds, off):
        fn = self.ctx.api.remove_day_off if off else self.ctx.api.add_day_off
        self.ctx.guard(lambda: fn(ds), then=lambda: self.refresh(soft=True))

    def add(self, ds):
        TaskDialog(self.ctx, ds, on_saved=lambda: self.refresh(soft=True)).open()

    def edit(self, occ):
        TaskDialog(self.ctx, occ["date"], occ=occ, on_saved=lambda: self.refresh(soft=True)).open()
