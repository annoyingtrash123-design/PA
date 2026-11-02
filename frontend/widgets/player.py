"""Voice player: fetches TTS, plays it with ft.Audio (src_base64), shows play/pause/stop."""
import base64

import flet as ft

import theme as T
from api import ApiError


class VoicePlayer(ft.Container):
    def __init__(self, ctx):
        super().__init__(visible=False, padding=ft.padding.symmetric(horizontal=6, vertical=2),
                         bgcolor=T.RAISED, border_radius=24)
        self.ctx = ctx
        self.audio = None
        self.playing = False
        self.play_btn = ft.IconButton(ft.Icons.PAUSE_CIRCLE_FILLED, icon_color=T.LAV, icon_size=34, on_click=self._toggle_play)
        self.stop_btn = ft.IconButton(ft.Icons.STOP_CIRCLE_OUTLINED, icon_color=T.MUTED, icon_size=30, on_click=self.stop)
        self.label = ft.Text("Playing...", size=13, color=T.MUTED)
        self.content = ft.Row([self.play_btn, self.stop_btn, self.label], spacing=2, vertical_alignment=ft.CrossAxisAlignment.CENTER)

    def speak(self, text, voice):
        """Fetch + play. Returns True on success; shows a gentle snackbar on failure."""
        try:
            data = self.ctx.api.tts(text, voice)
            if not data:
                raise ApiError("empty audio", 503)
        except ApiError:
            self.ctx.snack("Voice unavailable right now")
            return False
        self._load(base64.b64encode(data).decode())
        return True

    def _load(self, b64):
        page = self.ctx.page
        if self.audio is not None and self.audio in page.overlay:
            page.overlay.remove(self.audio)
        self.audio = ft.Audio(src_base64=b64, autoplay=True, on_state_changed=self._state)
        page.overlay.append(self.audio)
        page.update()
        self.playing = True
        self._show()

    def _state(self, e):
        st = str(e.data).lower()
        if "completed" in st or "stopped" in st:
            self.playing = False
            self.label.value = "Done"
            self.play_btn.icon = ft.Icons.PLAY_CIRCLE_FILLED
        elif "playing" in st:
            self.playing = True
            self.label.value = "Playing..."
            self.play_btn.icon = ft.Icons.PAUSE_CIRCLE_FILLED
        elif "paused" in st:
            self.playing = False
            self.label.value = "Paused"
            self.play_btn.icon = ft.Icons.PLAY_CIRCLE_FILLED
        try:
            self.update()
        except Exception:
            pass

    def _show(self):
        self.visible = True
        self.label.value = "Playing..."
        self.play_btn.icon = ft.Icons.PAUSE_CIRCLE_FILLED
        self.update()

    def _toggle_play(self, e):
        if self.audio is None:
            return
        if self.playing:
            self.audio.pause()
        else:
            # after completion/stop, resume() restarts from the current position; seek to start first
            self.audio.seek(0)
            self.audio.resume()

    def stop(self, e=None):
        if self.audio is not None:
            self.audio.pause()
            self.audio.seek(0)
        self.playing = False
        self.label.value = "Stopped"
        self.play_btn.icon = ft.Icons.PLAY_CIRCLE_FILLED
        self.update()
