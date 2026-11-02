from datetime import date, timedelta

import flet as ft

import theme as T
from views.tasks import monday_of
from widgets.base import BaseView
from widgets.common import card, chip, heading, muted
from widgets.player import VoicePlayer


class ReviewView(BaseView):
    def __init__(self, ctx):
        super().__init__(ctx)
        self.week = monday_of(date.today())
        self.player = VoicePlayer(ctx)
        self.text = ""

    def go(self, delta=None, reset=False):
        self.week = monday_of(date.today()) if reset else self.week + timedelta(days=7 * delta)
        self.player.visible = False
        self.refresh()

    def render(self):
        api = self.ctx.api
        days = api.days(self.week.isoformat(), 7)
        # weekly briefing is anchored on any date in the Mon-Sun week; use today if it is inside the week, else Monday
        anchor = date.today() if monday_of(date.today()) == self.week else self.week
        b = api.briefing("weekly", anchor.isoformat())
        self.text = b.get("text", "")
        end = self.week + timedelta(days=6)
        this = self.week == monday_of(date.today())
        header = ft.Row([
            ft.IconButton(ft.Icons.CHEVRON_LEFT, on_click=lambda e: self.go(-1), icon_color=T.INK),
            ft.Column([heading(f"{self.week.day} {self.week:%b} - {end.day} {end:%b}", 17),
                       muted("This week" if this else str(self.week.year), 12)], spacing=0,
                      horizontal_alignment=ft.CrossAxisAlignment.CENTER, expand=True),
            ft.IconButton(ft.Icons.CHEVRON_RIGHT, on_click=lambda e: self.go(1), icon_color=T.INK),
        ], alignment=ft.MainAxisAlignment.SPACE_BETWEEN)
        out = [header]
        if not this:
            out.append(ft.Row([ft.OutlinedButton("This week", icon=ft.Icons.TODAY, on_click=lambda e: self.go(reset=True),
                                                 style=ft.ButtonStyle(color=T.CORAL_DARK, shape=ft.StadiumBorder()))],
                              alignment=ft.MainAxisAlignment.CENTER))
        total = sum(len(d["items"]) for d in days)
        done = sum(1 for d in days for o in d["items"] if o["done"])
        stats = b.get("stats") or {}
        tiles = [self._tile("Tasks done", f"{done}/{total}", ft.Icons.CHECK_CIRCLE_OUTLINE)]
        if "gym_sessions" in stats:
            tiles.append(self._tile("Gym sessions", str(stats["gym_sessions"]), ft.Icons.FITNESS_CENTER))
        out.append(ft.Row(tiles, spacing=10))
        out.append(self._bars(days))
        out.append(card(ft.Column([
            ft.Row([ft.Icon(ft.Icons.AUTO_STORIES, color=T.CORAL, size=20), heading(f"{self.ctx.name}'s take", 16)], spacing=8),
            ft.Text(self.text, size=15, color=T.INK, selectable=True),
            ft.FilledButton("Read it to me", icon=ft.Icons.VOLUME_UP, on_click=self._read,
                            style=ft.ButtonStyle(shape=ft.StadiumBorder(), bgcolor=T.CORAL, color="#FFFFFF")),
            self.player], spacing=12)))
        return out

    def _tile(self, label, value, icon):
        return ft.Container(ft.Column([ft.Icon(icon, color=T.CORAL, size=22),
                                       ft.Text(value, size=26, weight=ft.FontWeight.W_700, color=T.INK), muted(label, 12)], spacing=2),
                            expand=True, padding=16, bgcolor=T.PEACH_SOFT, border_radius=20, border=ft.border.all(1, T.LINE))

    def _bars(self, days):
        cols = []
        for d in days:
            n = len(d["items"])
            k = sum(1 for o in d["items"] if o["done"])
            frac = (k / n) if n else 0
            cols.append(ft.Column([
                ft.Container(ft.Container(bgcolor=T.CORAL, height=60 * frac, border_radius=6), height=60, width=26,
                             bgcolor=T.PEACH, border_radius=6, alignment=ft.alignment.bottom_center),
                muted(d["weekday"][:3], 11), muted(f"{k}/{n}", 10)],
                spacing=2, horizontal_alignment=ft.CrossAxisAlignment.CENTER))
        return card(ft.Row(cols, alignment=ft.MainAxisAlignment.SPACE_AROUND))

    def _read(self, e):
        if self.text:
            self.player.speak(self.text, self.ctx.settings.get("voice") or "en-US-JennyNeural")
