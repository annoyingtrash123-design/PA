from datetime import date, timedelta

import flet as ft

import theme as T
from views.home import pretty_date
from widgets.base import BaseView
from widgets.common import OccurrenceTile, card, chip, heading, muted
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
            ft.IconButton(ft.Icons.CHEVRON_LEFT, on_click=lambda e: self.go(-1), icon_color=T.INK, tooltip="Previous week"),
            ft.Column([heading(rng, 17), muted("This week" if this else f"{self.week.year}", 12)], spacing=0,
                      horizontal_alignment=ft.CrossAxisAlignment.CENTER, expand=True),
            ft.IconButton(ft.Icons.CHEVRON_RIGHT, on_click=lambda e: self.go(1), icon_color=T.INK, tooltip="Next week"),
        ], alignment=ft.MainAxisAlignment.SPACE_BETWEEN)
        out = [header]
        if not this:
            out.append(ft.Row([ft.OutlinedButton("This week", icon=ft.Icons.TODAY, on_click=lambda e: self.go(reset=True),
                                                 style=ft.ButtonStyle(color=T.CORAL_DARK, shape=ft.StadiumBorder()))],
                              alignment=ft.MainAxisAlignment.CENTER))
        for d in days:
            out.append(self._day(d))
        return out

    def _day(self, d):
        dt = date.fromisoformat(d["date"])
        is_today = dt == date.today()
        title = ft.Text(f"{d['weekday']} {dt.day} {dt:%b}", size=16, weight=ft.FontWeight.W_700,
                        color=T.CORAL_DARK if is_today else T.INK)
        head = [title]
        if is_today:
            head.append(chip("Today", bgcolor=T.CORAL, color="#FFFFFF"))
        wk = d.get("workout")
        if wk:
            head.append(chip("Rest" if wk.get("rest") else wk["name"],
                             ft.Icons.SPA_OUTLINED if wk.get("rest") else ft.Icons.FITNESS_CENTER,
                             bgcolor="#E6F2E8" if wk.get("rest") else "#FFE6B8"))
        top = ft.Row([ft.Row(head, spacing=8, wrap=True, expand=True),
                      ft.IconButton(ft.Icons.ADD_CIRCLE, icon_color=T.CORAL, icon_size=30, tooltip="Add task",
                                    on_click=lambda e, ds=d["date"]: self.add(ds))],
                     vertical_alignment=ft.CrossAxisAlignment.CENTER)
        col = [top]
        if not d["items"]:
            col.append(muted("Nothing planned", 13))
        for o in d["items"]:
            col.append(OccurrenceTile(self.ctx, o, lambda: self.refresh(soft=True), on_open=self.edit))
        return card(ft.Column(col, spacing=4), bgcolor=T.PEACH_SOFT if is_today else T.CARD,
                    border_color=T.CORAL if is_today else T.LINE, padding=14)

    def add(self, ds):
        TaskDialog(self.ctx, ds, on_saved=lambda: self.refresh(soft=True)).open()

    def edit(self, occ):
        TaskDialog(self.ctx, occ["date"], occ=occ, on_saved=lambda: self.refresh(soft=True)).open()
