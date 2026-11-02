from datetime import date, datetime

import flet as ft

import theme as T
from api import ApiError
from widgets.base import BaseView
from widgets.common import OccurrenceTile, card, chip, heading, muted, btn_style
from widgets.player import VoicePlayer


def pretty_date(d):
    return f"{d:%A} {d.day} {d:%b}"


def greeting(now=None):
    h = (now or datetime.now()).hour
    return "Good morning" if h < 12 else "Good afternoon" if h < 18 else "Good evening"


def group_items(items):
    """Timed first (flat), then untimed grouped by part of day. -> list[(label|None, [occ])]"""
    timed = [o for o in items if o.get("time")]
    groups = [(None, timed)] if timed else []
    for part in T.PART_ORDER:
        g = [o for o in items if not o.get("time") and (o.get("part_of_day") or "anytime") == part]
        if g:
            groups.append((T.PART_LABELS[part], g))
    return groups


def weather_icon(summary):
    s = (summary or "").lower()
    if "rain" in s or "drizzle" in s or "shower" in s:
        return ft.Icons.UMBRELLA
    if "snow" in s:
        return ft.Icons.AC_UNIT
    if "cloud" in s or "overcast" in s or "fog" in s:
        return ft.Icons.CLOUD
    if "storm" in s or "thunder" in s:
        return ft.Icons.THUNDERSTORM
    return ft.Icons.WB_SUNNY


class HomeView(BaseView):
    def __init__(self, ctx):
        super().__init__(ctx)
        self.player = VoicePlayer(ctx)
        self.kind = "morning" if datetime.now().hour < 17 else "evening"
        self.brief_text = None
        self.brief_kind = None
        self.briefing_extra = None  # weather/affirmation from last briefing fetch
        self.busy = False

    def refresh(self, soft=False):
        if not soft:
            self.briefing_extra = None
        super().refresh(soft)

    # ---- data
    def render(self):
        today = date.today()
        day = self.ctx.api.days(today.isoformat(), 1)[0]
        if self.briefing_extra is None:
            try:
                self.briefing_extra = self.ctx.api.briefing("morning", today.isoformat(), ai=False)
            except ApiError:
                self.briefing_extra = {}
        items = day["items"]
        total, done = len(items), sum(1 for o in items if o["done"])
        nxt = next((o for o in items if not o["done"]), None)
        extra = self.briefing_extra or {}
        user = self.ctx.settings.get("user_name") or ""
        hello = f"{greeting()}, {user}" if user else greeting()

        controls = [self._header(hello, today, extra.get("weather"))]
        if extra.get("affirmation"):
            controls.append(self._affirmation(extra["affirmation"]))
        controls.append(self._progress(done, total, nxt))
        controls.append(self._brief_section())
        controls.append(self._workout(day.get("workout")))
        controls.append(self._list(items))
        return controls

    # ---- sections
    def _header(self, hello, today, weather):
        row = [ft.Column([ft.Text(hello, size=26, weight=ft.FontWeight.W_700, color=T.INK),
                          muted(pretty_date(today), 14)], spacing=2, expand=True)]
        col = [ft.Row(row)]
        if weather:
            col.append(ft.Row([chip(f"{weather.get('summary', '')}  {round(weather.get('temp_min', 0))}-{round(weather.get('temp_max', 0))}°C",
                                    weather_icon(weather.get("summary")), bgcolor="#FFE6B8"),
                               chip(self.ctx.settings.get("location_name") or "", ft.Icons.PLACE_OUTLINED)
                               if self.ctx.settings.get("location_name") else ft.Container()], spacing=8, wrap=True))
        return ft.Column(col, spacing=10)

    def _affirmation(self, text):
        return card(ft.Row([ft.Icon(ft.Icons.FAVORITE, color=T.CORAL, size=22),
                            ft.Text(text, size=15, italic=True, color=T.INK, expand=True)], spacing=12,
                           vertical_alignment=ft.CrossAxisAlignment.START), bgcolor=T.PEACH_SOFT)

    def _progress(self, done, total, nxt):
        ring = ft.Stack([
            ft.ProgressRing(value=(done / total) if total else 0, width=92, height=92, stroke_width=10,
                            color=T.CORAL, bgcolor=T.PEACH, stroke_cap=ft.StrokeCap.ROUND),
            ft.Container(ft.Column([ft.Text(f"{done}/{total}", size=20, weight=ft.FontWeight.W_700, color=T.INK)],
                                   alignment=ft.MainAxisAlignment.CENTER, horizontal_alignment=ft.CrossAxisAlignment.CENTER),
                         width=92, height=92, alignment=ft.alignment.center),
        ], width=92, height=92)
        if total == 0:
            nxt_col = [heading("Nothing planned", 16), muted("A blank canvas. Add something in Tasks.")]
        elif nxt is None:
            nxt_col = [heading("All done!", 16), muted("Beautiful work today. Time to relax.")]
        else:
            nxt_col = [muted("NEXT UP", 11), ft.Text(nxt["title"], size=17, weight=ft.FontWeight.W_600, color=T.INK, max_lines=2),
                       muted(nxt["time"] or T.PART_LABELS.get(nxt["part_of_day"], ""), 13)]
        return card(ft.Row([ring, ft.Column(nxt_col, spacing=3, expand=True)], spacing=18,
                           vertical_alignment=ft.CrossAxisAlignment.CENTER))

    def _brief_section(self):
        self.brief_btn = ft.FilledButton(
            "Brief me" if self.kind == "morning" else "Wind-down",
            icon=ft.Icons.WB_SUNNY_OUTLINED if self.kind == "morning" else ft.Icons.NIGHTLIGHT_OUTLINED,
            on_click=self._brief, height=58, expand=True,
            style=ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=29), bgcolor=T.CORAL, color="#FFFFFF",
                                 text_style=ft.TextStyle(size=18, weight=ft.FontWeight.W_600)))
        self.kind_sel = ft.SegmentedButton(
            selected={self.kind}, show_selected_icon=False, on_change=self._kind_changed,
            segments=[ft.Segment(value="morning", label=ft.Text("Morning")), ft.Segment(value="evening", label=ft.Text("Wind-down"))])
        self.brief_card_holder = ft.Container()
        self._set_brief_card()
        return ft.Column([ft.Row([self.brief_btn]), ft.Row([self.kind_sel], alignment=ft.MainAxisAlignment.CENTER),
                          self.brief_card_holder], spacing=10)

    def _set_brief_card(self):
        if not self.brief_text:
            self.brief_card_holder.content = None
            return
        title = "Your morning briefing" if self.brief_kind == "morning" else "Your wind-down"
        self.brief_card_holder.content = card(ft.Column([
            ft.Row([ft.Icon(ft.Icons.CHAT_BUBBLE_OUTLINE, color=T.CORAL, size=18), heading(f"{title} from {self.ctx.name}", 15)], spacing=8),
            ft.Text(self.brief_text, size=15, color=T.INK, selectable=True),
            self.player], spacing=10), bgcolor="#FFFFFF")

    def _kind_changed(self, e):
        self.kind = next(iter(e.control.selected))
        self.brief_btn.text = "Brief me" if self.kind == "morning" else "Wind-down"
        self.brief_btn.icon = ft.Icons.WB_SUNNY_OUTLINED if self.kind == "morning" else ft.Icons.NIGHTLIGHT_OUTLINED
        self.brief_btn.update()

    def _brief(self, e):
        if self.busy:
            return
        self.busy = True
        self.brief_btn.disabled = True
        self.brief_btn.text = "Thinking..."
        self.brief_btn.update()
        try:
            try:
                b = self.ctx.api.briefing(self.kind, date.today().isoformat())
            except ApiError as err:
                self.ctx.snack(err.message)
                return
            self.brief_text, self.brief_kind = b["text"], self.kind
            self.player.visible = False
            self._set_brief_card()
            self.brief_card_holder.update()
            self.player.speak(b["text"], self.ctx.settings.get("voice") or "en-US-JennyNeural")
        finally:
            self.busy = False
            self.brief_btn.disabled = False
            self.brief_btn.text = "Brief me" if self.kind == "morning" else "Wind-down"
            self.brief_btn.update()

    def _workout(self, wk):
        if not wk:
            return ft.Container()
        if wk.get("rest"):
            body = ft.Row([ft.Icon(ft.Icons.SPA_OUTLINED, color=T.GOOD), ft.Column([heading("Rest day", 16), muted("Recover well. You earned it.")], spacing=1, expand=True)], spacing=12)
        else:
            names = ", ".join(x["name"] for x in wk.get("exercises", [])[:3])
            body = ft.Row([ft.Icon(ft.Icons.FITNESS_CENTER, color=T.CORAL),
                           ft.Column([muted("TODAY'S WORKOUT", 11), heading(wk["name"], 17), muted(names, 12)], spacing=1, expand=True),
                           ft.Icon(ft.Icons.CHEVRON_RIGHT, color=T.INK_SOFT)], spacing=12)
        return card(body, on_click=lambda e: self.ctx.navigate(2))

    def _list(self, items):
        out = [heading("Today")]
        if not items:
            out.append(muted("No tasks today."))
        for label, group in group_items(items):
            inner = []
            if label:
                inner.append(muted(label.upper(), 11, weight=ft.FontWeight.W_600))
            inner += [OccurrenceTile(self.ctx, o, lambda: self.refresh(soft=True)) for o in group]
            out.append(card(ft.Column(inner, spacing=4), padding=12))
        return ft.Column(out, spacing=8)
