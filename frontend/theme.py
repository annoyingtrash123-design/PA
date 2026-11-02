"""Warm, calm Material 3 light theme + shared constants."""
import flet as ft

CREAM = "#FFF8F0"
PEACH = "#FFE8D6"
PEACH_SOFT = "#FFF1E4"
CORAL = "#EE7B5F"
CORAL_DARK = "#C9553A"
AMBER = "#F5A524"
INK = "#3D2B24"
INK_SOFT = "#7A6359"
CARD = "#FFFFFF"
LINE = "#F0DFD0"
GOOD = "#6BAA75"

PALETTE = ["#EC6A5C", "#F59E0B", "#E8B931", "#6BAA75", "#4CAF50", "#26A69A",
           "#4FA3D1", "#5C6BC0", "#9575CD", "#D5679B", "#A1887F", "#78909C"]

PART_LABELS = {"morning": "Morning", "afternoon": "Afternoon", "evening": "Evening", "anytime": "Anytime"}
PART_ORDER = ["morning", "afternoon", "evening", "anytime"]
WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]


def make_theme():
    return ft.Theme(
        use_material3=True,
        color_scheme=ft.ColorScheme(
            primary=CORAL, on_primary="#FFFFFF", primary_container=PEACH, on_primary_container=INK,
            secondary=AMBER, secondary_container="#FFE6B8", on_secondary_container=INK,
            surface=CREAM, on_surface=INK, surface_container=PEACH_SOFT, surface_container_high=PEACH,
            on_surface_variant=INK_SOFT, outline=LINE, outline_variant=LINE,
            error="#C0392B",
        ),
        navigation_bar_theme=ft.NavigationBarTheme(
            bgcolor="#FFFFFF", indicator_color=PEACH,
            label_text_style=ft.TextStyle(size=12, color=INK),
        ),
        scaffold_bgcolor=CREAM,
        font_family="Roboto",
    )
