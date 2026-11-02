"""Warm dark Material 3 theme (lavender + sage accents) + shared constants."""
import flet as ft

BG = "#2A2725"          # page background (warm dark grey)
CARD = "#35312E"        # cards / surfaces
RAISED = "#3E3935"      # raised cards, inputs
LINE = "#4A4440"        # borders
TEXT = "#EEE8E2"
MUTED = "#B5ABA3"
LAV = "#B8A6E3"         # primary accent
LAV_SOFT = "#4A4360"    # chips / selected backgrounds
SAGE = "#9CB89A"        # secondary accent (done, success)
SAGE_SOFT = "#3C4A3B"
ON_ACCENT = "#2A2725"   # dark text on lavender / sage
ERROR = "#E8998D"

PALETTE = ["#EC6A5C", "#F59E0B", "#E8B931", "#6BAA75", "#4CAF50", "#26A69A",
           "#4FA3D1", "#5C6BC0", "#9575CD", "#D5679B", "#A1887F", "#78909C"]

PART_LABELS = {"morning": "Morning", "afternoon": "Afternoon", "evening": "Evening", "anytime": "Anytime"}
PART_ORDER = ["morning", "afternoon", "evening", "anytime"]
WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]


def make_theme():
    return ft.Theme(
        use_material3=True,
        color_scheme=ft.ColorScheme(
            primary=LAV, on_primary=ON_ACCENT, primary_container=LAV_SOFT, on_primary_container=TEXT,
            secondary=SAGE, on_secondary=ON_ACCENT, secondary_container=SAGE_SOFT, on_secondary_container=TEXT,
            tertiary=SAGE, on_tertiary=ON_ACCENT,
            surface=BG, on_surface=TEXT, on_surface_variant=MUTED,
            surface_tint=BG, surface_container_lowest=BG, surface_container_low=CARD, surface_container=CARD,
            surface_container_high=RAISED, surface_container_highest=RAISED,
            outline=MUTED, outline_variant=LINE, inverse_surface=TEXT, on_inverse_surface=BG, inverse_primary=LAV_SOFT,
            error=ERROR, on_error=ON_ACCENT, shadow="#000000", scrim="#000000",
        ),
        navigation_bar_theme=ft.NavigationBarTheme(
            bgcolor=CARD, indicator_color=LAV_SOFT, surface_tint_color=CARD,
            label_text_style=ft.TextStyle(size=12, color=TEXT),
        ),
        dialog_theme=ft.DialogTheme(bgcolor=BG),
        scaffold_bgcolor=BG,
        font_family="Roboto",
    )
