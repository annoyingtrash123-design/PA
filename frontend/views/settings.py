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
        self.f_tz = tf("Timezone", s.get("timezone") or "Australia/Sydney", helper_text="e.g. Australia/Sydney")
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
        memory_card = self._memory_card()
        return [
            heading("Settings", 22),
            card(ft.Column([heading("About you", 16), self.f_assistant, self.f_user], spacing=12)),
            card(ft.Column([heading("Voice", 16), self.f_voice,
                            ft.OutlinedButton("Preview voice", icon=ft.Icons.RECORD_VOICE_OVER, on_click=self._preview,
                                              style=ft.ButtonStyle(shape=ft.StadiumBorder(), color=T.LAV)),
                            self.player], spacing=12)),
            card(ft.Column([heading("Weather location", 16), self.f_loc, ft.Row([self.f_lat, self.f_lon], spacing=10), self.f_tz], spacing=12)),
            card(ft.Column([heading("AI (Gemini)", 16), self.f_key], spacing=12)),
            memory_card,
            card(ft.Column([heading("Categories", 16), self.cat_col,
                            ft.Row([self.new_dot, self.new_cat, ft.IconButton(ft.Icons.ADD_CIRCLE, icon_color=T.LAV, icon_size=30,
                                                                              on_click=self._add_cat)], spacing=4)], spacing=10)),
            ft.FilledButton("Save settings", icon=ft.Icons.CHECK, on_click=self._save, height=52, style=btn_style(26)),
        ]

    # ---- assistant memory
    def _memory_card(self):
        try:
            mem = self.ctx.api.memory() or {}
        except ApiError as err:
            return card(ft.Column([heading(f"What {self.ctx.name} knows about you", 16), muted(err.message)], spacing=8))
        w = mem.get("weather") or {}
        self.notes = list(mem.get("notes") or [])

        def num(label, key, default):
            return ft.TextField(label=label, value=str(w.get(key, default)), suffix_text="°C", border_radius=14, dense=True,
                                keyboard_type=ft.KeyboardType.NUMBER)
        self.w_comfy = num("Comfortable up to", "comfortable_max", 18)
        self.w_hot = num("Hot above", "hot_above", 22)
        self.w_toohot = num("Too hot above", "too_hot_above", 25)
        self.w_rain = ft.Switch(label="I like rainy days", value=bool(w.get("likes_rain")), active_color=T.LAV)
        self.w_sun = ft.Switch(label="I want to avoid sunburn", value=bool(w.get("avoid_sunburn")), active_color=T.LAV)
        wk = mem.get("work") or {}
        self.work_days = set(wk.get("days", [0, 1, 2, 3, 4]))
        self.wk_on = ft.Switch(label="I work set hours", value=bool(wk.get("enabled", True)), active_color=T.LAV)
        self.wk_chips = ft.Row(spacing=6, wrap=True, run_spacing=6)
        self._day_chips()
        self.wk_start = ft.TextField(label="Start", value=wk.get("start", "10:00"), border_radius=14, dense=True, expand=True,
                                     hint_text="HH:MM")
        self.wk_end = ft.TextField(label="End", value=wk.get("end", "17:00"), border_radius=14, dense=True, expand=True,
                                   hint_text="HH:MM")
        self.wk_lunch = ft.TextField(label="Lunch break", value=str(wk.get("lunch_minutes", 60)), suffix_text="min", border_radius=14,
                                     dense=True, keyboard_type=ft.KeyboardType.NUMBER)
        rh = mem.get("rhythm") or {}

        def tm(label, key, default):
            return ft.TextField(label=label, value=rh.get(key, default), border_radius=14, dense=True, expand=True, hint_text="HH:MM")
        self.rh_wf, self.rh_wt = tm("From", "wake_from", "06:00"), tm("To", "wake_to", "07:00")
        self.rh_sf, self.rh_st = tm("From", "sleep_from", "23:00"), tm("To", "sleep_to", "00:00")
        self.wk_region = ft.TextField(label="Holiday region", value=wk.get("holiday_region", "AU-NSW"), border_radius=14,
                                      dense=True, helper_text="Public holidays are shown for this region, e.g. AU-NSW")
        self.notes_col = ft.Column(spacing=2)
        self._notes()
        self.new_note = ft.TextField(hint_text="Add a note", helper_text="e.g. I work from the office Tue–Thu", border_radius=14,
                                     dense=True, expand=True, on_submit=self._add_note)
        name = self.ctx.name
        return card(ft.Column([
            heading(f"What {name} knows about you", 16),
            muted(f"{name} uses this to write your daily digest and briefings.", 13),
            muted("WEATHER PREFERENCES", 11, weight=ft.FontWeight.W_600),
            ft.Column([self.w_comfy, self.w_hot, self.w_toohot], spacing=10),
            self.w_rain, self.w_sun,
            ft.OutlinedButton("Save weather preferences", icon=ft.Icons.CHECK, on_click=self._save_weather,
                              style=ft.ButtonStyle(shape=ft.StadiumBorder(), color=T.LAV)),
            ft.Divider(color=T.LINE, height=8),
            muted("WORK HOURS", 11, weight=ft.FontWeight.W_600),
            self.wk_on, self.wk_chips,
            ft.Row([self.wk_start, self.wk_end], spacing=8),
            self.wk_lunch, self.wk_region,
            ft.OutlinedButton("Save work hours", icon=ft.Icons.CHECK, on_click=self._save_work,
                              style=ft.ButtonStyle(shape=ft.StadiumBorder(), color=T.LAV)),
            ft.Divider(color=T.LINE, height=8),
            muted("DAILY RHYTHM", 11, weight=ft.FontWeight.W_600),
            ft.Text("Usually wake", size=14, color=T.TEXT),
            ft.Row([self.rh_wf, self.rh_wt], spacing=8),
            ft.Text("Usually asleep by", size=14, color=T.TEXT),
            ft.Row([self.rh_sf, self.rh_st], spacing=8),
            ft.OutlinedButton("Save daily rhythm", icon=ft.Icons.CHECK, on_click=self._save_rhythm,
                              style=ft.ButtonStyle(shape=ft.StadiumBorder(), color=T.LAV)),
            ft.Divider(color=T.LINE, height=8),
            muted("NOTES", 11, weight=ft.FontWeight.W_600),
            self.notes_col,
            ft.Row([self.new_note, ft.IconButton(ft.Icons.ADD_CIRCLE, icon_color=T.LAV, icon_size=30, tooltip="Add note",
                                                 on_click=self._add_note)], spacing=4, vertical_alignment=ft.CrossAxisAlignment.START),
        ], spacing=10))

    def _day_chips(self):
        self.wk_chips.controls = [
            ft.Container(ft.Text(n, size=13, weight=ft.FontWeight.W_500, color=T.ON_ACCENT if i in self.work_days else T.TEXT),
                         bgcolor=T.LAV if i in self.work_days else T.RAISED, border_radius=16,
                         border=ft.border.all(1, T.LAV if i in self.work_days else T.LINE),
                         padding=ft.padding.symmetric(horizontal=12, vertical=7), on_click=lambda e, i=i: self._toggle_day(i))
            for i, n in enumerate(T.WEEKDAYS)]

    def _toggle_day(self, i):
        self.work_days ^= {i}
        self._day_chips()
        self.wk_chips.update()

    def _save_work(self, e):
        import re
        st, en = self.wk_start.value.strip(), self.wk_end.value.strip()
        if not (re.fullmatch(r"([01]\d|2[0-3]):[0-5]\d", st) and re.fullmatch(r"([01]\d|2[0-3]):[0-5]\d", en)):
            return self.ctx.snack("Times should look like 09:30")
        try:
            lunch = int(float(self.wk_lunch.value))
        except (TypeError, ValueError):
            return self.ctx.snack("Lunch break should be a number of minutes")
        body = {"enabled": bool(self.wk_on.value), "lunch_minutes": lunch, "days": sorted(self.work_days), "start": st, "end": en,
                "holiday_region": self.wk_region.value.strip() or "AU-NSW"}
        if self.ctx.guard(lambda: self.ctx.api.save_memory_work(body)) is not None:
            self.ctx.snack("Work hours saved")

    def _save_rhythm(self, e):
        import re
        keys = [("wake_from", self.rh_wf), ("wake_to", self.rh_wt), ("sleep_from", self.rh_sf), ("sleep_to", self.rh_st)]
        body = {k: f.value.strip() for k, f in keys}
        if not all(re.fullmatch(r"([01]\d|2[0-3]):[0-5]\d", v) for v in body.values()):
            return self.ctx.snack("Times should look like 06:30")
        if self.ctx.guard(lambda: self.ctx.api.save_memory_rhythm(body)) is not None:
            self.ctx.snack("Daily rhythm saved")

    def _notes(self):
        if not self.notes:
            self.notes_col.controls = [muted("No notes yet.")]
            return
        self.notes_col.controls = [
            ft.Row([ft.Text(n["text"], size=14.5, color=T.TEXT, expand=True),
                    ft.IconButton(ft.Icons.EDIT_OUTLINED, icon_color=T.MUTED, icon_size=20, tooltip="Edit",
                                  on_click=lambda e, n=n: self._edit_note(n)),
                    ft.IconButton(ft.Icons.DELETE_OUTLINE, icon_color=T.MUTED, icon_size=20, tooltip="Delete",
                                  on_click=lambda e, n=n: self._del_note(n))],
                   spacing=0, vertical_alignment=ft.CrossAxisAlignment.CENTER) for n in self.notes]

    def _set_notes(self, mem):
        if mem is None:
            return
        self.notes = list(mem.get("notes") or [])
        self._notes()
        self.notes_col.update()

    def _save_weather(self, e):
        try:
            body = {"comfortable_max": float(self.w_comfy.value), "hot_above": float(self.w_hot.value),
                    "too_hot_above": float(self.w_toohot.value),
                    "likes_rain": bool(self.w_rain.value), "avoid_sunburn": bool(self.w_sun.value)}
        except (TypeError, ValueError):
            return self.ctx.snack("Temperatures should be numbers")
        body = {k: (int(v) if isinstance(v, float) and v == int(v) else v) for k, v in body.items()}
        if self.ctx.guard(lambda: self.ctx.api.save_memory_weather(body)) is not None:
            self.ctx.snack("Weather preferences saved")

    def _add_note(self, e):
        text = (self.new_note.value or "").strip()
        if not text:
            return
        mem = self.ctx.guard(lambda: self.ctx.api.add_note(text))
        if mem is not None:
            self.new_note.value = ""
            self.new_note.update()
            self._set_notes(mem)

    def _del_note(self, n):
        self._set_notes(self.ctx.guard(lambda: self.ctx.api.delete_note(n["id"])))

    def _edit_note(self, n):
        field = ft.TextField(value=n["text"], multiline=True, border_radius=14, autofocus=True)

        def save(e):
            text = (field.value or "").strip()
            if not text:
                return
            self.ctx.close(dlg)
            self._set_notes(self.ctx.guard(lambda: self.ctx.api.update_note(n["id"], text)))
        dlg = ft.AlertDialog(title=ft.Text("Edit note"), bgcolor=T.RAISED, shape=ft.RoundedRectangleBorder(radius=24),
                             content=ft.Container(field, width=320),
                             actions=[ft.TextButton("Cancel", on_click=lambda e: self.ctx.close(dlg)),
                                      ft.FilledButton("Save", on_click=save)])
        self.ctx.open(dlg)

    # ---- categories
    def _cats(self):
        rows = []
        for c in self.ctx.categories:
            name = ft.TextField(value=c["name"], dense=True, border_radius=12, expand=True,
                                on_blur=lambda e, c=c: self._rename(c, e.control.value), on_submit=lambda e, c=c: self._rename(c, e.control.value))
            rows.append(ft.Row([ft.Container(dot(c["color"], 26), on_click=lambda e, c=c: self._pick_color(c), padding=4), name,
                                ft.IconButton(ft.Icons.DELETE_OUTLINE, icon_color=T.MUTED, on_click=lambda e, c=c: self._del_cat(c))],
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
        dlg = ft.AlertDialog(title=ft.Text("Pick a colour"), bgcolor=T.RAISED, shape=ft.RoundedRectangleBorder(radius=24),
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
                "voice": self.f_voice.value, "location_name": self.f_loc.value.strip(),
                "timezone": self.f_tz.value.strip() or "Australia/Sydney"}
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
