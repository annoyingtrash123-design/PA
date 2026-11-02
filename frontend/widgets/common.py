"""Shared UI building blocks."""
from datetime import datetime

import flet as ft

import theme as T
from api import ApiError


def now_in(ctx):
    """Current naive datetime in the user's timezone (settings.timezone), local time if unset/invalid."""
    tz = (getattr(ctx, "settings", None) or {}).get("timezone")
    if tz:
        try:
            from zoneinfo import ZoneInfo
            return datetime.now(ZoneInfo(tz)).replace(tzinfo=None)
        except Exception:
            pass
    return datetime.now()


def card(content, padding=16, bgcolor=T.CARD, on_click=None, margin=None, border_color=T.LINE, radius=20):
    return ft.Container(
        content=content, padding=padding, bgcolor=bgcolor, border_radius=radius,
        border=ft.border.all(1, border_color), on_click=on_click, margin=margin,
        shadow=ft.BoxShadow(blur_radius=12, color="#59000000", offset=ft.Offset(0, 3)),
    )


def heading(text, size=18):
    return ft.Text(text, size=size, weight=ft.FontWeight.W_600, color=T.TEXT)


def muted(text, size=13, **kw):
    return ft.Text(text, size=size, color=T.MUTED, **kw)


def dot(color, size=10):
    return ft.Container(width=size, height=size, border_radius=size, bgcolor=color or T.LINE)


def chip(text, icon=None, bgcolor=T.LAV_SOFT, color=T.TEXT, on_click=None):
    row = []
    if icon:
        row.append(ft.Icon(icon, size=15, color=color))
    row.append(ft.Text(text, size=12.5, color=color, weight=ft.FontWeight.W_500))
    return ft.Container(
        content=ft.Row(row, spacing=5, tight=True), padding=ft.padding.symmetric(horizontal=10, vertical=5),
        bgcolor=bgcolor, border_radius=14, on_click=on_click)


def loading():
    return ft.Container(ft.ProgressRing(color=T.LAV), alignment=ft.alignment.center, padding=40)


def error_card(err, on_retry):
    msg = err.message if isinstance(err, ApiError) else f"Something went wrong ({err})"
    return card(
        ft.Column([
            ft.Text("Hmm, I can't reach my brain right now", size=17, weight=ft.FontWeight.W_600, color=T.TEXT),
            muted(msg),
            muted("Make sure the backend is running, then try again."),
            ft.FilledButton("Retry", icon=ft.Icons.REFRESH, on_click=lambda e: on_retry(), style=btn_style()),
        ], spacing=10, horizontal_alignment=ft.CrossAxisAlignment.START),
        bgcolor=T.RAISED)


def btn_style(radius=24):
    return ft.ButtonStyle(shape=ft.RoundedRectangleBorder(radius=radius), padding=ft.padding.symmetric(horizontal=22, vertical=14))


def hhmm(t):
    return t or ""


def time_or_part(o):
    return o["time"] if o.get("time") else T.PART_LABELS.get(o.get("part_of_day") or "anytime", "Anytime")


class OccurrenceTile(ft.Column):
    """One task occurrence: checkbox, colour dot, title, carried-over tag, expandable checklist."""

    def __init__(self, ctx, occ, on_changed, on_open=None, compact=False):
        super().__init__(spacing=0)
        self.ctx, self.occ, self.on_changed, self.on_open = ctx, occ, on_changed, on_open
        self.expanded = False
        o = occ
        color = (o.get("category") or {}).get("color")
        self.cb = ft.Checkbox(value=o["done"], on_change=self._toggle, active_color=T.LAV, shape=ft.CircleBorder(), scale=1.1)
        title = ft.Text(o["title"], size=15, weight=ft.FontWeight.W_500, color=T.MUTED if o["done"] else T.TEXT,
                        style=ft.TextStyle(decoration=ft.TextDecoration.LINE_THROUGH if o["done"] else None),
                        max_lines=2, overflow=ft.TextOverflow.ELLIPSIS)
        sub = [muted(time_or_part(o), 12)]
        if o.get("rolled_over_from"):
            sub.append(chip("carried over", ft.Icons.REDO, bgcolor=T.LAV_SOFT))
        if o.get("recurring"):
            sub.append(ft.Icon(ft.Icons.REPEAT, size=13, color=T.MUTED))
        text_col = ft.Column([title, ft.Row(sub, spacing=6, wrap=True)], spacing=2, expand=True)
        row = [self.cb, dot(color, 10), ft.Container(text_col, expand=True, on_click=(lambda e: on_open(o)) if on_open else None)]
        self.arrow = None
        if o.get("checklist"):
            n_done = sum(1 for c in o["checklist"] if c["done"])
            self.arrow = ft.TextButton(f"{n_done}/{len(o['checklist'])}", icon=ft.Icons.EXPAND_MORE, on_click=self._expand,
                                       style=ft.ButtonStyle(color=T.MUTED, padding=4))
            row.append(self.arrow)
        self.checks = ft.Column(
            [ft.Row([ft.Checkbox(label=c["text"], value=c["done"], active_color=T.LAV,
                                 on_change=lambda e, c=c: self._toggle_item(c, e.control.value))], spacing=0)
             for c in o.get("checklist", [])], spacing=0, visible=False)
        self.controls = [ft.Row(row, spacing=6, vertical_alignment=ft.CrossAxisAlignment.CENTER),
                         ft.Container(self.checks, padding=ft.padding.only(left=44))]

    def _expand(self, e):
        self.expanded = not self.expanded
        self.checks.visible = self.expanded
        self.arrow.icon = ft.Icons.EXPAND_LESS if self.expanded else ft.Icons.EXPAND_MORE
        self.update()

    def _toggle(self, e):
        self.ctx.guard(lambda: self.ctx.api.toggle(self.occ["task_id"], self.occ["date"], self.cb.value), then=self.on_changed)

    def _toggle_item(self, c, val):
        self.ctx.guard(lambda: self.ctx.api.toggle_check(self.occ["task_id"], self.occ["date"], c["id"], val), then=self.on_changed)
