import flet as ft

import theme as T
from api import ApiError
from widgets.base import BaseView
from widgets.common import btn_style, card, chip, heading, muted

SUBS = [("today", "Today"), ("rotation", "Rotation"), ("templates", "Templates"), ("progress", "Progress")]


def fmt_w(w):
    if w is None or w == "":
        return ""
    try:
        w = float(w)
    except (TypeError, ValueError):
        return str(w)
    return str(int(w)) if w == int(w) else str(w)


def num(s, cast=float, default=0):
    try:
        return cast(str(s).replace(",", ".").strip())
    except ValueError:
        return default


class GymView(BaseView):
    def __init__(self, ctx):
        super().__init__(ctx)
        self.sub = "today"
        self.session = None      # active workout session dict
        self.plan = None
        self.fields = {}         # (exercise_id, set_no) -> (weight_tf, reps_tf, done_cb)
        self.progress_ex = None
        self.new_ex_name = ""

    # ---------- shell
    def render(self):
        seg = ft.SegmentedButton(
            selected={self.sub}, show_selected_icon=False, allow_empty_selection=False,
            segments=[ft.Segment(value=k, label=ft.Text(v, size=12)) for k, v in SUBS],
            on_change=self._sub_changed, style=ft.ButtonStyle(padding=ft.padding.symmetric(horizontal=4)))
        body = {"today": self.r_today, "rotation": self.r_rotation, "templates": self.r_templates, "progress": self.r_progress}[self.sub]()
        return [ft.Row([seg], alignment=ft.MainAxisAlignment.CENTER)] + body

    def _sub_changed(self, e):
        self.sub = next(iter(e.control.selected))
        self.refresh()

    # ---------- today
    def r_today(self):
        api = self.ctx.api
        if self.session and not self.session.get("completed"):
            return self.r_workout()
        plan = api.gym_next()
        self.plan = plan
        if plan.get("rest"):
            return [card(ft.Column([
                ft.Row([ft.Icon(ft.Icons.SPA_OUTLINED, color=T.SAGE, size=28), heading("Next up: Rest day", 18)], spacing=10),
                muted("No lifting today. Stretch, walk, nap, whatever feels good."),
                ft.FilledButton("Mark rest day", icon=ft.Icons.CHECK, on_click=self._rest, style=btn_style())], spacing=12))]
        out = [heading(f"Next workout: {plan['name']}", 20)]
        for ex in plan.get("exercises", []):
            lines = [ft.Row([heading(ex["name"], 16), chip(f"{ex['sets']} x {ex['rep_min']}-{ex['rep_max']}")],
                            alignment=ft.MainAxisAlignment.SPACE_BETWEEN)]
            if ex.get("last"):
                sets = ", ".join(f"{fmt_w(s['weight'])}kg x {s['reps']}" for s in ex["last"]["sets"])
                lines.append(muted(f"Last time ({ex['last']['date']}): {sets}", 12.5))
            else:
                lines.append(muted("No history yet. Pick a weight that feels right.", 12.5))
            if ex.get("hint"):
                lines.append(ft.Row([ft.Icon(ft.Icons.TRENDING_UP, size=16, color=T.SAGE),
                                     ft.Text(ex["hint"], size=13, color=T.TEXT, expand=True)], spacing=6))
            out.append(card(ft.Column(lines, spacing=5), padding=14))
        out.append(ft.FilledButton("Start workout", icon=ft.Icons.PLAY_ARROW, height=56, on_click=self._start,
                                   style=ft.ButtonStyle(shape=ft.StadiumBorder(), bgcolor=T.LAV, color=T.ON_ACCENT,
                                                        text_style=ft.TextStyle(size=17, weight=ft.FontWeight.W_600))))
        return out

    def _rest(self, e):
        if self.ctx.guard(lambda: self.ctx.api.gym_rest()) is not None:
            self.ctx.snack("Rest day logged. Enjoy it!")
            self.refresh()

    def _start(self, e):
        s = self.ctx.guard(lambda: self.ctx.api.start_session(self.plan["template_id"]))
        if s:
            self.session = s
            self.refresh()

    # ---------- workout mode
    def r_workout(self):
        s = self.session
        names = {x["id"]: x["name"] for x in self.ctx.api.exercises()}
        plan_ex = {e["exercise_id"]: e for e in (self.plan or {}).get("exercises", [])}
        order, by_ex = [], {}
        for st in s["sets"]:
            if st["exercise_id"] not in by_ex:
                order.append(st["exercise_id"])
                by_ex[st["exercise_id"]] = []
            by_ex[st["exercise_id"]].append(st)
        self.fields = {}
        out = [ft.Row([heading(s["name"], 20), chip("In progress", ft.Icons.TIMER_OUTLINED, bgcolor=T.LAV_SOFT)],
                      alignment=ft.MainAxisAlignment.SPACE_BETWEEN)]
        for exid in order:
            pe = plan_ex.get(exid, {})
            head = [heading(names.get(exid, pe.get("name", f"Exercise {exid}")), 16)]
            if pe:
                head.append(muted(f"{pe['rep_min']}-{pe['rep_max']} reps", 12))
            col = [ft.Row(head, alignment=ft.MainAxisAlignment.SPACE_BETWEEN)]
            if pe.get("last"):
                col.append(muted("Last: " + ", ".join(f"{fmt_w(x['weight'])}x{x['reps']}" for x in pe["last"]["sets"]), 12))
            if pe.get("hint"):
                col.append(ft.Text(pe["hint"], size=12.5, color=T.SAGE))
            for st in by_ex[exid]:
                w = ft.TextField(value=fmt_w(st.get("weight")), suffix_text="kg", dense=True, border_radius=12, width=110,
                                 keyboard_type=ft.KeyboardType.NUMBER, text_align=ft.TextAlign.CENTER, content_padding=8,
                                 on_blur=lambda e: self._save_sets())
                r = ft.TextField(value=str(st.get("reps", "")), suffix_text="reps", dense=True, border_radius=12, width=110,
                                 keyboard_type=ft.KeyboardType.NUMBER, text_align=ft.TextAlign.CENTER, content_padding=8,
                                 on_blur=lambda e: self._save_sets())
                d = ft.Checkbox(value=st.get("done", False), active_color=T.SAGE, shape=ft.CircleBorder(), scale=1.3,
                                on_change=lambda e: self._save_sets(notify=True))
                self.fields[(exid, st["set_no"])] = (w, r, d)
                col.append(ft.Row([muted(f"Set {st['set_no']}", 13, width=44), w, r, d], spacing=6,
                                  vertical_alignment=ft.CrossAxisAlignment.CENTER))
            out.append(card(ft.Column(col, spacing=8), padding=14))
        out.append(ft.FilledButton("Finish workout", icon=ft.Icons.FLAG, height=56, on_click=self._finish,
                                   style=ft.ButtonStyle(shape=ft.StadiumBorder(), bgcolor=T.LAV, color=T.ON_ACCENT,
                                                        text_style=ft.TextStyle(size=17, weight=ft.FontWeight.W_600))))
        out.append(ft.TextButton("Back (keeps the workout)", on_click=self._leave, style=ft.ButtonStyle(color=T.MUTED)))
        return out

    def _collect(self):
        sets = []
        for st in self.session["sets"]:
            w, r, d = self.fields.get((st["exercise_id"], st["set_no"]), (None, None, None))
            if w is None:
                sets.append(st)
                continue
            st["weight"] = num(w.value, float, 0)
            st["reps"] = num(r.value, int, 0)
            st["done"] = bool(d.value)
            sets.append(st)
        return sets

    def _save_sets(self, notify=False):
        if not self.session:
            return None
        sets = self._collect()
        res = self.ctx.guard(lambda: self.ctx.api.save_sets(self.session["id"], sets), error_msg="Couldn't save that set, check your connection")
        if res:
            self.session = res
        return res

    def _leave(self, e):
        self._save_sets()
        self.session = None
        self.refresh()

    def _finish(self, e):
        if self._save_sets() is None:
            return
        done = self.ctx.guard(lambda: self.ctx.api.complete_session(self.session["id"]))
        if done is None:
            return
        n_done = sum(1 for s in done["sets"] if s.get("done"))
        name = done.get("name", "workout")
        self.session = None
        dlg = ft.AlertDialog(
            modal=True, bgcolor=T.RAISED, shape=ft.RoundedRectangleBorder(radius=24),
            title=ft.Row([ft.Icon(ft.Icons.CELEBRATION, color=T.LAV), ft.Text("Workout done!")], spacing=8),
            content=ft.Text(f"You smashed {name}: {n_done} sets logged. Go get a snack and be proud of yourself!", color=T.TEXT),
            actions=[ft.FilledButton("Yay", on_click=lambda ev: (self.ctx.close(dlg), self.refresh()), style=btn_style())])
        self.ctx.open(dlg)
        self.refresh()

    # ---------- rotation
    def r_rotation(self):
        rot = self.ctx.api.rotation()
        tmpls = {t["id"]: t["name"] for t in self.ctx.api.templates()}
        items = rot["items"]
        nxt = rot.get("next_index", 0)
        rows = []
        for i, it in enumerate(items):
            label = "Rest" if it.get("rest") else tmpls.get(it.get("template_id"), "?")
            is_next = i == nxt
            rows.append(ft.Container(ft.Row([
                ft.Container(ft.Text(str(i + 1), size=13, weight=ft.FontWeight.W_600, color=T.ON_ACCENT if is_next else T.TEXT),
                             width=28, height=28, border_radius=14, bgcolor=T.LAV if is_next else T.LAV_SOFT, alignment=ft.alignment.center),
                ft.Column([ft.Text(label, size=16, weight=ft.FontWeight.W_500, color=T.TEXT),
                           muted("You are here: next up", 11.5) if is_next else ft.Container(height=0)], spacing=0, expand=True),
                ft.IconButton(ft.Icons.ARROW_UPWARD, icon_size=20, disabled=i == 0, on_click=lambda e, i=i: self._move(items, i, -1)),
                ft.IconButton(ft.Icons.ARROW_DOWNWARD, icon_size=20, disabled=i == len(items) - 1, on_click=lambda e, i=i: self._move(items, i, 1)),
                ft.IconButton(ft.Icons.DELETE_OUTLINE, icon_size=20, on_click=lambda e, i=i: self._rot_del(items, i)),
            ], spacing=2, vertical_alignment=ft.CrossAxisAlignment.CENTER),
                bgcolor=T.RAISED if is_next else None, border_radius=14, padding=ft.padding.symmetric(horizontal=6, vertical=2)))
        self.rot_add = ft.Dropdown(label="Add to cycle", border_radius=14, expand=True,
                                   options=[ft.dropdown.Option(f"t{k}", v) for k, v in tmpls.items()] + [ft.dropdown.Option("rest", "Rest day")])
        return [heading("Your cycle", 20), muted("Workouts follow this order and loop around."),
                card(ft.Column(rows or [muted("Empty. Add a workout below.")], spacing=2), padding=10),
                ft.Row([self.rot_add, ft.FilledButton("Add", on_click=lambda e: self._rot_add(items), style=btn_style())], spacing=8)]

    def _put_rot(self, items):
        if self.ctx.guard(lambda: self.ctx.api.save_rotation(items)) is not None:
            self.refresh(soft=True)

    def _move(self, items, i, d):
        items = list(items)
        items[i], items[i + d] = items[i + d], items[i]
        self._put_rot(items)

    def _rot_del(self, items, i):
        self._put_rot([x for k, x in enumerate(items) if k != i])

    def _rot_add(self, items):
        v = self.rot_add.value
        if not v:
            return
        self._put_rot(list(items) + [{"rest": True} if v == "rest" else {"template_id": int(v[1:])}])

    # ---------- templates
    def r_templates(self):
        tmpls = self.ctx.api.templates()
        out = [ft.Row([heading("Templates", 20), ft.FilledTonalButton("New", icon=ft.Icons.ADD, on_click=lambda e: self._edit_template(None))],
                      alignment=ft.MainAxisAlignment.SPACE_BETWEEN)]
        for t in tmpls:
            out.append(card(ft.Column([heading(t["name"], 16),
                                       muted(", ".join(f"{x['name']} {x['sets']}x{x['rep_min']}-{x['rep_max']}" for x in t["exercises"]) or "No exercises yet", 12.5)],
                                      spacing=3), padding=14, on_click=lambda e, t=t: self._edit_template(t)))
        if not tmpls:
            out.append(muted("No templates yet."))
        return out

    def _edit_template(self, t):
        try:
            exercises = self.ctx.api.exercises()
        except ApiError as e:
            return self.ctx.snack(e.message)
        rows = [dict(x) for x in (t["exercises"] if t else [])]
        name = ft.TextField(label="Template name", value=t["name"] if t else "", border_radius=14)
        col = ft.Column(spacing=8)
        picker = ft.Dropdown(label="Add exercise", border_radius=14,
                             options=[ft.dropdown.Option(str(x["id"]), x["name"]) for x in exercises] + [ft.dropdown.Option("new", "+ New exercise...")])
        new_name = ft.TextField(label="New exercise name", border_radius=14, visible=False)
        err = ft.Text("", color=T.ERROR, size=13, visible=False)

        def rebuild():
            col.controls = []
            for i, r in enumerate(rows):
                def mk(key, label):
                    f = ft.TextField(value=str(r[key]), label=label, dense=True, border_radius=10, width=64, content_padding=8,
                                     keyboard_type=ft.KeyboardType.NUMBER, text_align=ft.TextAlign.CENTER)
                    f.on_change = lambda e, r=r, key=key: r.__setitem__(key, num(e.control.value, int, r[key]))
                    return f
                col.controls.append(ft.Container(ft.Column([
                    ft.Row([ft.Text(r["name"], size=14.5, weight=ft.FontWeight.W_500, color=T.TEXT, expand=True),
                            ft.IconButton(ft.Icons.ARROW_UPWARD, icon_size=18, disabled=i == 0, on_click=lambda e, i=i: swap(i, -1)),
                            ft.IconButton(ft.Icons.ARROW_DOWNWARD, icon_size=18, disabled=i == len(rows) - 1, on_click=lambda e, i=i: swap(i, 1)),
                            ft.IconButton(ft.Icons.CLOSE, icon_size=18, on_click=lambda e, i=i: remove(i))], spacing=0),
                    ft.Row([mk("sets", "Sets"), mk("rep_min", "Min"), mk("rep_max", "Max")], spacing=8)], spacing=4),
                    bgcolor=T.RAISED, padding=10, border_radius=14))

        def swap(i, d):
            rows[i], rows[i + d] = rows[i + d], rows[i]
            rebuild(); col.update()

        def remove(i):
            rows.pop(i); rebuild(); col.update()

        def picked(e):
            new_name.visible = picker.value == "new"
            new_name.update()

        picker.on_change = picked

        def add_ex(e):
            v = picker.value
            if not v:
                return
            if v == "new":
                nm = (new_name.value or "").strip()
                if not nm:
                    return
                ex = self.ctx.guard(lambda: self.ctx.api.add_exercise(nm))
                if not ex:
                    return
                exercises.append(ex)
                picker.options.insert(len(picker.options) - 1, ft.dropdown.Option(str(ex["id"]), ex["name"]))
            else:
                ex = next(x for x in exercises if str(x["id"]) == v)
            rows.append({"exercise_id": ex["id"], "name": ex["name"], "sets": 3, "rep_min": 8, "rep_max": 12})
            picker.value = None
            new_name.value = ""
            new_name.visible = False
            rebuild()
            dlg.content.content.update()

        def save(e):
            nm = (name.value or "").strip()
            if not nm:
                err.value, err.visible = "Give the template a name.", True
                return err.update()
            body = {"name": nm, "exercises": [{k: r[k] for k in ("exercise_id", "sets", "rep_min", "rep_max")} for r in rows]}
            try:
                self.ctx.api.update_template(t["id"], body) if t else self.ctx.api.add_template(body)
            except ApiError as ex:
                err.value, err.visible = ex.message, True
                return err.update()
            self.ctx.close(dlg)
            self.refresh(soft=True)

        def delete(e):
            if self.ctx.guard(lambda: self.ctx.api.delete_template(t["id"])) is not None:
                self.ctx.close(dlg)
                self.refresh(soft=True)

        rebuild()
        dlg = ft.AlertDialog(
            modal=True, bgcolor=T.RAISED, shape=ft.RoundedRectangleBorder(radius=24),
            title=ft.Text("Edit template" if t else "New template", weight=ft.FontWeight.W_600),
            content=ft.Container(ft.Column([name, col, picker, new_name,
                                            ft.TextButton("Add", icon=ft.Icons.ADD, on_click=add_ex), err],
                                           spacing=10, tight=True), width=340), scrollable=True,
            actions=([ft.TextButton("Delete", on_click=delete, style=ft.ButtonStyle(color=T.ERROR))] if t else []) +
                    [ft.TextButton("Cancel", on_click=lambda e: self.ctx.close(dlg), style=ft.ButtonStyle(color=T.MUTED)),
                     ft.FilledButton("Save", on_click=save, style=btn_style())],
            inset_padding=ft.padding.symmetric(horizontal=12, vertical=16))
        self.ctx.open(dlg)

    # ---------- progress
    def r_progress(self):
        exercises = self.ctx.api.exercises()
        if self.progress_ex is None and exercises:
            self.progress_ex = str(exercises[0]["id"])
        dd = ft.Dropdown(label="Exercise", value=self.progress_ex, border_radius=14,
                         options=[ft.dropdown.Option(str(x["id"]), x["name"]) for x in exercises],
                         on_change=self._ex_changed)
        out = [heading("Progress", 20), dd]
        if not self.progress_ex:
            return out + [muted("Add some exercises first.")]
        hist = self.ctx.api.history(int(self.progress_ex))
        if not hist:
            return out + [card(muted("No sessions logged for this one yet. Go lift something!"))]
        ws = [h["top_weight"] for h in hist]
        lo, hi = min(ws), max(ws)
        span = max(hi - lo, 1)
        step = 1 if span <= 4 else 2.5 if span <= 12 else 5 if span <= 30 else 10
        y_min = max(0, (int((lo - step * 0.5) // step)) * step)
        y_max = (int((hi + step * 0.5) // step) + 1) * step
        ticks = []
        v = y_min
        while v <= y_max:
            ticks.append(v)
            v += step
        pts = [ft.LineChartDataPoint(i, h["top_weight"]) for i, h in enumerate(hist)]
        step_x = max(1, len(hist) // 4)
        chart = ft.LineChart(
            data_series=[ft.LineChartData(data_points=pts, stroke_width=4, color=T.LAV, curved=True, stroke_cap_round=True,
                                          point=True, below_line_bgcolor="#33B8A6E3")],
            min_x=-0.3, max_x=len(hist) - 0.7 if len(hist) > 1 else 0.7, min_y=y_min, max_y=y_max,
            left_axis=ft.ChartAxis(labels_size=40, labels_interval=step, labels=[ft.ChartAxisLabel(value=v, label=ft.Text(fmt_w(v), size=10, color=T.MUTED))
                                                                                 for v in ticks]),
            bottom_axis=ft.ChartAxis(labels_size=24, labels_interval=step_x, labels=[ft.ChartAxisLabel(value=i, label=ft.Text(hist[i]["date"][5:], size=10, color=T.MUTED))
                                                             for i in range(0, len(hist), step_x)]),
            horizontal_grid_lines=ft.ChartGridLines(interval=step, color=T.LINE, width=1), tooltip_bgcolor=T.TEXT, height=200, expand=True)
        out.append(card(ft.Column([muted("Top weight (kg)", 12), chart], spacing=8), padding=14))
        entries = [ft.Row([ft.Text(h["date"], size=14, color=T.TEXT, expand=True),
                           ft.Text(f"{fmt_w(h['top_weight'])} kg x {h.get('best_reps', '-')}", size=14, weight=ft.FontWeight.W_500, color=T.TEXT),
                           muted(f"1RM ~{fmt_w(h.get('est_1rm'))}", 12)]) for h in reversed(hist)]
        out.append(card(ft.Column([heading("Entries", 15)] + entries, spacing=8), padding=14))
        return out

    def _ex_changed(self, e):
        self.progress_ex = e.control.value
        self.refresh()
