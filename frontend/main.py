"""Personal assistant frontend (Flet 0.28.x). Run: python main.py"""
import os

import flet as ft

import theme as T
from api import Api, ApiError, BACKEND_URL
from views.goals import GoalsView
from views.gym import GymView
from views.home import HomeView
from views.review import ReviewView
from views.settings import SettingsView
from views.tasks import TasksView


class Ctx:
    """Shared app context passed to every view."""

    def __init__(self, page: ft.Page):
        self.page = page
        self.api = Api(BACKEND_URL)
        self.settings = {"assistant_name": "Sunny", "user_name": "", "voice": "en-US-JennyNeural"}
        self.categories = []
        self.navigate = lambda i: None

    @property
    def name(self):
        return self.settings.get("assistant_name") or "Sunny"

    def load_meta(self):
        """Fetch settings + categories; raises ApiError."""
        self.settings = self.api.settings()
        self.categories = self.api.categories()

    def snack(self, msg):
        self.page.open(ft.SnackBar(ft.Text(msg, color=T.TEXT), bgcolor=T.RAISED, behavior=ft.SnackBarBehavior.FLOATING,
                                   shape=ft.RoundedRectangleBorder(radius=14)))

    def guard(self, fn, then=None, error_msg=None):
        """Run an API call; on ApiError show a friendly snackbar. Calls then(result) on success."""
        try:
            res = fn()
        except ApiError as e:
            self.snack(error_msg or e.message)
            return None
        if then:
            then()
        return res

    def open(self, dlg):
        self.page.open(dlg)

    def close(self, dlg):
        self.page.close(dlg)


def main(page: ft.Page):
    page.title = "Sunny"
    page.theme = T.make_theme()
    page.theme_mode = ft.ThemeMode.DARK
    page.bgcolor = T.BG
    page.padding = 0
    page.spacing = 0

    ctx = Ctx(page)
    views = [HomeView(ctx), TasksView(ctx), GoalsView(ctx), GymView(ctx), ReviewView(ctx)]
    settings_view = SettingsView(ctx)
    state = {"tab": 0, "settings": False}

    body = ft.Container(expand=True, padding=ft.padding.symmetric(horizontal=16))
    holder = ft.Row([ft.Container(body, width=640)], alignment=ft.MainAxisAlignment.CENTER, expand=True, vertical_alignment=ft.CrossAxisAlignment.STRETCH)
    gear = ft.IconButton(ft.Icons.SETTINGS_OUTLINED, icon_color=T.TEXT, tooltip="Settings")
    titlebox = ft.Text("Sunny", size=22, weight=ft.FontWeight.W_700, color=T.LAV)

    def sync_title():
        titlebox.value = ctx.name
        page.title = ctx.name

    def show():
        if state["settings"]:
            v = settings_view
            page.appbar.leading = ft.IconButton(ft.Icons.ARROW_BACK, icon_color=T.TEXT, on_click=lambda e: close_settings())
            gear.visible = False
        else:
            v = views[state["tab"]]
            page.appbar.leading = None
            gear.visible = True
        body.content = v.root
        sync_title()
        page.update()
        v.refresh()

    def close_settings():
        state["settings"] = False
        show()

    def open_settings(e):
        state["settings"] = True
        show()

    def on_nav(e):
        state["tab"] = e.control.selected_index
        state["settings"] = False
        show()

    def navigate(i):
        nav.selected_index = i
        state["tab"] = i
        state["settings"] = False
        show()

    ctx.navigate = navigate
    ctx.on_settings_saved = lambda: (sync_title(), page.update())
    gear.on_click = open_settings

    page.appbar = ft.AppBar(title=titlebox, center_title=False, bgcolor=T.BG, elevation=0,
                            actions=[gear, ft.Container(width=4)], leading_width=48)
    nav = ft.NavigationBar(
        selected_index=0, on_change=on_nav, bgcolor=T.CARD, indicator_color=T.LAV_SOFT,
        destinations=[
            ft.NavigationBarDestination(icon=ft.Icons.HOME_OUTLINED, selected_icon=ft.Icons.HOME, label="Home"),
            ft.NavigationBarDestination(icon=ft.Icons.CHECK_CIRCLE_OUTLINE, selected_icon=ft.Icons.CHECK_CIRCLE, label="Tasks"),
            ft.NavigationBarDestination(icon=ft.Icons.FLAG_OUTLINED, selected_icon=ft.Icons.FLAG, label="Goals"),
            ft.NavigationBarDestination(icon=ft.Icons.FITNESS_CENTER_OUTLINED, selected_icon=ft.Icons.FITNESS_CENTER, label="Gym"),
            ft.NavigationBarDestination(icon=ft.Icons.AUTO_STORIES_OUTLINED, selected_icon=ft.Icons.AUTO_STORIES, label="Review"),
        ])
    page.navigation_bar = nav

    def on_resize(e=None):
        w = page.width or 400
        holder.controls[0].width = min(w, 640)
        try:
            holder.update()
        except Exception:
            pass

    page.on_resize = on_resize
    page.add(holder)
    on_resize()
    try:
        ctx.load_meta()
    except ApiError:
        pass  # views show their own retry card
    show()


if __name__ == "__main__":
    ft.app(target=main, view=ft.AppView.WEB_BROWSER, port=int(os.getenv("PORT", 8550)), host="0.0.0.0")
