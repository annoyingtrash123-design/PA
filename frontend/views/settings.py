import flet as ft

import theme as T
from api import ApiError
from widgets.base import BaseView
from widgets.common import btn_style, card, dot, heading, muted
from widgets.player import VoicePlayer

VOICES = [("en-US-JennyNeural", "Jenny"), ("en-US-AriaNeural", "Aria")]


def tf(label, value="", **kw):
    return ft.TextField(label=label, value=value, border_radius=14, **kw)


class SettingsView(BaseView):
    def __init__(self, ctx):
        super().__init__(ctx)
        self.player = VoicePlayer(ctx)

    def render(self):
        s = self.ctx.settings
        self.f_assistant = tf("Assistant name", s.get("assistant_name", ""))
        self.f_user = tf("Your name", s.get("user_name", ""))
        self.f_voice = ft.Dropdown(label="Voice", value=s.get("voice") or VOICES[0][0], border_radius=14,
                                   options=[ft.dropdown.Option(v, n) for v, n in VOICES])
        self.f_loc = tf("Location name", s.get("location_name", ""))
        self.f_lat = tf("Latitude", str(s.get("latitude", "")), expand=True, keyboard_type=ft.KeyboardType.NUMBER)
        self.f_lon = tf("Longitude", str(s.get("longitude", "")), expand=True, keyboard_type=ft.KeyboardType.NUMBER)
        key_set = s.get("gemini_api_key_set")
        self.f_key = tf("Gemini API key", password=True, can_reveal_password=True,
                        hint_text="Key saved" if key_set else "Paste your key",
                        helper_text="Key saved. Leave empty to keep it." if key_set else "Optional: powers chattier briefings.")
        self.cat_col = ft.Column(spacing=6)
        self._cats()
        self.new_cat = tf("New category", dense=True, expand=True)
        self.new_color = T.PALETTE[0]
        self.new_dot = ft.Container(dot(self.new_color, 26), on_click=lambda e: self._pick_color(None), padding=4)
        return [
            heading("Settings", 22),
            card(ft.Column([heading("About you", 16), self.f_assistant, self.f_user], spacing=12)),
            card(ft.Column([heading("Voice", 16), self.f_voice,
                            ft.OutlinedButton("Preview voice", icon=ft.Icons.RECORD_VOICE_OVER, on_click=self._preview,
                                              style=ft.ButtonStyle(shape=ft.StadiumBorder(), color=T.CORAL_DARK)),
                            self.player], spacing=12)),
            card(ft.Column([heading("Weather location", 16), self.f_loc, ft.Row([self.f_lat, self.f_lon], spacing=10)], spacing=12)),
            card(ft.Column([heading("AI (Gemini)", 16), self.f_key], spacing=12)),
            card(ft.Column([heading("Categories", 16), self.cat_col,
                            ft.Row([self.new_dot, self.new_cat, ft.IconButton(ft.Icons.ADD_CIRCLE, icon_color=T.CORAL, icon_size=30,
                                                                              on_click=self._add_cat)], spacing=4)], spacing=10)),
            ft.FilledButton("Save settings", icon=ft.Icons.CHECK, on_click=self._save, height=52, style=btn_style(26)),
        ]

    # ---- categories
    def _cats(self):
        rows = []
        for c in self.ctx.categories:
            name = ft.TextField(value=c["name"], dense=True, border_radius=12, expand=True,
                                on_blur=lambda e, c=c: self._rename(c, e.control.value), on_submit=lambda e, c=c: self._rename(c, e.control.value))
            rows.append(ft.Row([ft.Container(dot(c["color"], 26), on_click=lambda e, c=c: self._pick_color(c), padding=4), name,
                                ft.IconButton(ft.Icons.DELETE_OUTLINE, icon_color=T.INK_SOFT, on_click=lambda e, c=c: self._del_cat(c))],
                               spacing=4))
        self.cat_col.controls = rows

    def _reload_cats(self):
        self.ctx.categories = self.ctx.api.categories()
        self._cats()
        self.cat_col.update()

    def _rename(self, c, value):
        value = (value or "").strip()
        if not value or value == c["name"]:
            return
        if self.ctx.guard(lambda: self.ctx.api.update_category(c["id"], value, c["color"])) is not None:
            c["name"] = value
            self.ctx.snack("Category renamed")

    def _pick_color(self, c):
        def choose(color):
            self.ctx.close(dlg)
            if c is None:
                self.new_color = color
                self.new_dot.content = dot(color, 26)
                self.new_dot.update()
            else:
                if self.ctx.guard(lambda: self.ctx.api.update_category(c["id"], c["name"], color)) is not None:
                    c["color"] = color
                    self._reload_cats()
        swatches = [ft.Container(width=44, height=44, bgcolor=col, border_radius=22, on_click=lambda e, col=col: choose(col))
                    for col in T.PALETTE]
        dlg = ft.AlertDialog(title=ft.Text("Pick a colour"), bgcolor=T.CREAM, shape=ft.RoundedRectangleBorder(radius=24),
                             content=ft.Container(ft.Row(swatches, wrap=True, spacing=10, run_spacing=10), width=300))
        self.ctx.open(dlg)

    def _add_cat(self, e):
        name = (self.new_cat.value or "").strip()
        if not name:
            return
        if self.ctx.guard(lambda: self.ctx.api.add_category(name, self.new_color)) is not None:
            self.new_cat.value = ""
            self.new_cat.update()
            self._reload_cats()

    def _del_cat(self, c):
        if self.ctx.guard(lambda: self.ctx.api.delete_category(c["id"])) is not None:
            self._reload_cats()

    # ---- actions
    def _preview(self, e):
        name = self.f_assistant.value or "Sunny"
        self.player.speak(f"Hi! I'm {name}. This is how I'll sound when I brief you.", self.f_voice.value)

    def _save(self, e):
        body = {"assistant_name": self.f_assistant.value.strip() or "Sunny", "user_name": self.f_user.value.strip(),
                "voice": self.f_voice.value, "location_name": self.f_loc.value.strip()}
        try:
            if self.f_lat.value.strip():
                body["latitude"] = float(self.f_lat.value)
            if self.f_lon.value.strip():
                body["longitude"] = float(self.f_lon.value)
        except ValueError:
            return self.ctx.snack("Latitude and longitude should be numbers")
        if self.f_key.value:
            body["gemini_api_key"] = self.f_key.value
        res = self.ctx.guard(lambda: self.ctx.api.save_settings(body))
        if res is not None:
            self.ctx.settings = res
            self.f_key.value = ""
            self.f_key.hint_text = "Key saved" if res.get("gemini_api_key_set") else "Paste your key"
            self.f_key.update()
            self.ctx.on_settings_saved()
            self.ctx.snack("Saved. Looking good!")
