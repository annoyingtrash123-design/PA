import flet as ft

from api import ApiError
from widgets.common import error_card, loading


class BaseView:
    """A tab view. Subclasses implement render() -> list[Control] (may raise ApiError)."""

    def __init__(self, ctx):
        self.ctx = ctx
        self.root = ft.Column(scroll=ft.ScrollMode.AUTO, expand=True, spacing=14, alignment=ft.MainAxisAlignment.START,
                              horizontal_alignment=ft.CrossAxisAlignment.STRETCH)

    def render(self):
        raise NotImplementedError

    def refresh(self, soft=False):
        if not soft:
            self.root.controls = [ft.Column([loading()])]
            self.safe_update()
        try:
            if not soft or not self.ctx.categories:
                try:
                    self.ctx.load_meta()
                except ApiError:
                    if not soft:
                        raise
            controls = self.render()
        except ApiError as e:
            controls = [error_card(e, self.refresh)]
        except Exception as e:  # keep UI alive, show friendly card
            import traceback
            traceback.print_exc()
            controls = [error_card(e, self.refresh)]
        controls = [ft.Container(height=4)] + controls + [ft.Container(height=24)]
        self.root.controls = [ft.Column(controls, spacing=14, horizontal_alignment=ft.CrossAxisAlignment.STRETCH)]
        self.safe_update()

    def safe_update(self):
        try:
            self.root.update()
        except Exception:
            pass
