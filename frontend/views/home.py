from datetime import date, datetime

import flet as ft

import theme as T
from api import ApiError
from widgets.base import BaseView
from widgets.common import card, now_in, chip, heading, muted, btn_style
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


def pick_next_up(items, now=None):
    """First not-done item still upcoming; else untimed item for current/later part of day; else any remaining."""
    now = now or datetime.now()
    left = [o for o in items if not o["done"]]
    hhmm = f"{now:%H:%M}"
    for o in left:
        if o.get("time") and o["time"] >= hhmm:
            return o
    cur = 0 if now.hour < 12 else 1 if now.hour < 17 else 2
    for o in left:
        if not o.get("time"):
            part = o.get("part_of_day") or "anytime"
            if part == "anytime" or T.PART_ORDER.index(part) >= cur:
                return o
    untimed = [o for o in left if not o.get("time")]
    return untimed[0] if untimed else (left[0] if left else None)


class HomeView(BaseView):
    def __init__(self, ctx):
        super().__init__(ctx)
        self.player = VoicePlayer(ctx)
        self.kind = "morning" if now_in(ctx).hour < 17 else "evening"
        self.brief_text = None
        self.overview = None
        self.busy = False

    def refresh(self, soft=False):
        super().refresh(soft)
        self._load_digest()
        self._load_briefing()

    # ---- data
    def render(self):
        today = now_in(self.ctx).date()
        day = self.ctx.api.days(today.isoformat(), 1)[0]
        items = day["items"]
        total, done = len(items), sum(1 for o in items if o["done"])
        nxt = pick_next_up(items, now_in(self.ctx))
        user = self.ctx.settings.get("user_name") or ""
        hello = f"{greeting(now_in(self.ctx))}, {user}" if user else greeting(now_in(self.ctx))
        self.weather_holder = ft.Container(visible=False)
        self.day_holder = ft.Container()
        self.play_btn = ft.FilledButton(
            "Read it to me", icon=ft.Icons.PLAY_ARROW_ROUNDED, on_click=self._play, height=58, expand=True, disabled=True,
            style=ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=29), bgcolor=T.LAV, color=T.ON_ACCENT,
                                 text_style=ft.TextStyle(size=18, weight=ft.FontWeight.W_600)))
        self.kind_sel = ft.SegmentedButton(
            selected={self.kind}, show_selected_icon=False, on_change=self._kind_changed,
            segments=[ft.Segment(value="morning", label=ft.Text("Morning")), ft.Segment(value="evening", label=ft.Text("Wind-down"))])
        self.brief_text = None
        self._set_day_card(loading=True)
        return [self._header(hello, today), self.weather_holder, self._progress(done, total, nxt),
                ft.Row([self.kind_sel], alignment=ft.MainAxisAlignment.CENTER), self.day_holder]

    def _loading_row(self):
        return ft.Row([ft.ProgressRing(width=18, height=18, stroke_width=2.5, color=T.LAV),
                       muted(f"{self.ctx.name} is looking at your day...", 14)], spacing=12)

    def _load_digest(self):
        try:
            dg = self.ctx.api.digest(now_in(self.ctx).date().isoformat()) or {}
        except ApiError:
            dg = {}
        self.overview = dg.get("overview")
        w = dg.get("weather")
        self.weather_holder.content = self._weather_card(w, dg.get("weather_comment")) if w else None
        self.weather_holder.visible = bool(w)
        try:
            self.weather_holder.update()
        except Exception:
            pass

    def _load_briefing(self):
        kind = self.kind
        self.brief_text = None
        self._set_day_card(loading=True)
        self._upd_day()
        try:
            b = self.ctx.api.briefing(kind, now_in(self.ctx).date().isoformat())
            text = (b or {}).get("text")
        except ApiError:
            text = None
        if kind != self.kind:  # user switched meanwhile
            return
        self.brief_text = text
        self.play_btn.disabled = not (text or self.overview)
        self._set_day_card()
        self._upd_day()

    def _upd_day(self):
        try:
            self.day_holder.update()
        except Exception:
            pass

    # ---- sections
    def _header(self, hello, today):
        return ft.Column([ft.Text(hello, size=26, weight=ft.FontWeight.W_700, color=T.TEXT),
                          muted(pretty_date(today), 14)], spacing=2)

    def _weather_card(self, w, comment):
        chips = [chip(f"{round(w.get('temp_min', 0))}-{round(w.get('temp_max', 0))}°C", bgcolor=T.LAV_SOFT)]
        if (w.get("uv_max") or 0) >= 3:
            chips.append(chip(f"UV {round(w['uv_max'])}", ft.Icons.WB_SUNNY_OUTLINED, bgcolor=T.SAGE_SOFT))
        if (w.get("precip_prob") or 0) >= 40:
            chips.append(chip(f"rain {round(w['precip_prob'])}%", ft.Icons.WATER_DROP_OUTLINED, bgcolor=T.SAGE_SOFT))
        loc = self.ctx.settings.get("location_name")
        top = ft.Row([ft.Icon(weather_icon(w.get("summary")), color=T.LAV, size=26),
                      ft.Column([ft.Text(w.get("summary", ""), size=16, weight=ft.FontWeight.W_600, color=T.TEXT),
                                 muted(loc, 12) if loc else ft.Container()], spacing=0, expand=True)],
                     spacing=12, vertical_alignment=ft.CrossAxisAlignment.CENTER)
        col = [top, ft.Row(chips, spacing=8, wrap=True, run_spacing=6)]
        if comment:
            col.append(ft.Text(comment, size=16, color=T.TEXT, style=ft.TextStyle(height=1.45)))
        return card(ft.Column(col, spacing=12), bgcolor=T.RAISED)

    def _progress(self, done, total, nxt):
        ring = ft.Stack([
            ft.ProgressRing(value=(done / total) if total else 0, width=64, height=64, stroke_width=7,
                            color=T.LAV, bgcolor=T.LAV_SOFT, stroke_cap=ft.StrokeCap.ROUND),
            ft.Container(ft.Column([ft.Text(f"{done}/{total}", size=15, weight=ft.FontWeight.W_700, color=T.TEXT)],
                                   alignment=ft.MainAxisAlignment.CENTER, horizontal_alignment=ft.CrossAxisAlignment.CENTER),
                         width=64, height=64, alignment=ft.alignment.center),
        ], width=64, height=64)
        if total == 0:
            nxt_col = [heading("Nothing planned", 16), muted("A blank canvas. Add something in Tasks.")]
        elif nxt is None:
            nxt_col = [heading("All done!", 16), muted("Beautiful work today. Time to relax.")]
        else:
            nxt_col = [muted("NEXT UP", 11), ft.Text(nxt["title"], size=17, weight=ft.FontWeight.W_600, color=T.TEXT, max_lines=2),
                       muted(nxt["time"] or T.PART_LABELS.get(nxt["part_of_day"], ""), 13)]
        return card(ft.Row([ring, ft.Column(nxt_col, spacing=3, expand=True)], spacing=16,
                           vertical_alignment=ft.CrossAxisAlignment.CENTER), padding=14)

    def _paragraphs(self, text):
        paras = [p.strip() for p in text.replace("\r", "").split("\n\n") if p.strip()]
        if len(paras) <= 1 and len(text) > 500:
            import re
            sents = re.split(r"(?<=[.!?])\s+", text.strip())
            n = max(3, min(5, round(len(sents) / 4)))
            size = -(-len(sents) // n)
            paras = [" ".join(sents[i:i + size]) for i in range(0, len(sents), size)]
        return paras

    def _set_day_card(self, loading=False):
        if loading:
            self.day_holder.content = card(self._loading_row(), bgcolor=T.RAISED)
            return
        text = self.brief_text or self.overview
        title = "Your day" if self.kind == "morning" else "Your wind-down"
        if not text:
            text = "I couldn't put your day into words just now. Check the Tasks tab for what's on."
        col = [ft.Row([ft.Icon(ft.Icons.WB_TWILIGHT if self.kind == "morning" else ft.Icons.NIGHTLIGHT_OUTLINED, color=T.LAV, size=20),
                       muted(f"{title.upper()} WITH {self.ctx.name.upper()}", 11, weight=ft.FontWeight.W_600)], spacing=8)]
        col += [ft.Text(p, size=16, color=T.TEXT, style=ft.TextStyle(height=1.5), selectable=True) for p in self._paragraphs(text)]
        col += [ft.Row([self.play_btn]), self.player]
        self.day_holder.content = card(ft.Column(col, spacing=14), bgcolor=T.RAISED)

    def _kind_changed(self, e):
        self.kind = next(iter(e.control.selected))
        self.player.visible = False
        self._load_briefing()

    def _play(self, e):
        text = self.brief_text or self.overview
        if self.busy or not text:
            return
        self.busy = True
        self.play_btn.disabled = True
        self.play_btn.text = "Getting ready..."
        self.play_btn.update()
        try:
            self.player.speak(text, self.ctx.settings.get("voice") or "en-US-JennyNeural")
        finally:
            self.busy = False
            self.play_btn.disabled = False
            self.play_btn.text = "Read it to me"
            self.play_btn.update()
