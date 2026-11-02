"""Thin httpx client for the backend (see docs/API.md)."""
import os

import httpx

BACKEND_URL = os.getenv("BACKEND_URL", "http://localhost:8080").rstrip("/")


class ApiError(Exception):
    """Raised for any failure talking to the backend. `.status` is None when unreachable."""

    def __init__(self, message, status=None):
        super().__init__(message)
        self.message = message
        self.status = status


class Api:
    def __init__(self, base=BACKEND_URL):
        self.base = base
        self.http = httpx.Client(base_url=base, timeout=20.0)

    def _req(self, method, path, **kw):
        try:
            r = self.http.request(method, path, **kw)
        except httpx.HTTPError as e:
            raise ApiError(f"Can't reach the backend at {self.base}", None) from e
        if r.status_code >= 400:
            msg = f"Backend error ({r.status_code})"
            try:
                msg = r.json().get("error", msg)
            except Exception:
                pass
            raise ApiError(msg, r.status_code)
        return r

    def _json(self, method, path, **kw):
        r = self._req(method, path, **kw)
        if not r.content:
            return None
        try:
            return r.json()
        except ValueError as e:
            raise ApiError("Unexpected response from backend", r.status_code) from e

    # settings / categories
    def settings(self): return self._json("GET", "/api/settings")
    def save_settings(self, body): return self._json("PUT", "/api/settings", json=body)
    def categories(self): return self._json("GET", "/api/categories")
    def add_category(self, name, color): return self._json("POST", "/api/categories", json={"name": name, "color": color})
    def update_category(self, cid, name, color): return self._json("PUT", f"/api/categories/{cid}", json={"name": name, "color": color})
    def delete_category(self, cid): return self._json("DELETE", f"/api/categories/{cid}")

    # tasks
    def get_task(self, tid): return self._json("GET", f"/api/tasks/{tid}")
    def add_task(self, task): return self._json("POST", "/api/tasks", json=task)
    def update_task(self, tid, task): return self._json("PUT", f"/api/tasks/{tid}", json=task)
    def delete_task(self, tid): return self._json("DELETE", f"/api/tasks/{tid}")
    def days(self, start, n=7): return self._json("GET", "/api/days", params={"start": start, "days": n})
    def toggle(self, task_id, date, done): return self._json("POST", "/api/occurrences/toggle", json={"task_id": task_id, "date": date, "done": done})
    def toggle_check(self, task_id, date, item_id, done):
        return self._json("POST", "/api/occurrences/checklist", json={"task_id": task_id, "date": date, "item_id": item_id, "done": done})

    # goals
    def goals(self): return self._json("GET", "/api/goals")
    def add_goal(self, g): return self._json("POST", "/api/goals", json=g)
    def update_goal(self, gid, g): return self._json("PUT", f"/api/goals/{gid}", json=g)
    def delete_goal(self, gid): return self._json("DELETE", f"/api/goals/{gid}")

    # gym
    def exercises(self): return self._json("GET", "/api/exercises")
    def add_exercise(self, name): return self._json("POST", "/api/exercises", json={"name": name})
    def templates(self): return self._json("GET", "/api/templates")
    def add_template(self, t): return self._json("POST", "/api/templates", json=t)
    def update_template(self, tid, t): return self._json("PUT", f"/api/templates/{tid}", json=t)
    def delete_template(self, tid): return self._json("DELETE", f"/api/templates/{tid}")
    def rotation(self): return self._json("GET", "/api/rotation")
    def save_rotation(self, items): return self._json("PUT", "/api/rotation", json={"items": items})
    def gym_next(self): return self._json("GET", "/api/gym/next")
    def gym_rest(self): return self._json("POST", "/api/gym/rest")
    def start_session(self, template_id): return self._json("POST", "/api/sessions", json={"template_id": template_id})
    def save_sets(self, sid, sets): return self._json("PUT", f"/api/sessions/{sid}/sets", json={"sets": sets})
    def complete_session(self, sid): return self._json("POST", f"/api/sessions/{sid}/complete")
    def history(self, exercise_id): return self._json("GET", f"/api/exercises/{exercise_id}/history")

    # briefing / tts
    def briefing(self, kind, date=None, ai=True):
        params = {"kind": kind}
        if not ai:
            params["ai"] = "false"
        if date:
            params["date"] = date
        return self._json("GET", "/api/briefing", params=params)

    def day_note(self, date):
        return self._json("GET", "/api/day-note", params={"date": date})

    def digest(self, date):
        return self._json("GET", "/api/digest", params={"date": date})

    # assistant memory
    def memory(self): return self._json("GET", "/api/memory")
    def save_memory_weather(self, w): return self._json("PUT", "/api/memory/weather", json=w)
    def save_memory_work(self, w): return self._json("PUT", "/api/memory/work", json=w)
    def save_memory_rhythm(self, r): return self._json("PUT", "/api/memory/rhythm", json=r)
    def days_off(self): return self._json("GET", "/api/days-off")
    def add_day_off(self, d): return self._json("POST", "/api/days-off", json={"date": d})
    def remove_day_off(self, d): return self._json("DELETE", f"/api/days-off/{d}")
    def add_note(self, text): return self._json("POST", "/api/memory/notes", json={"text": text})
    def update_note(self, nid, text): return self._json("PUT", f"/api/memory/notes/{nid}", json={"text": text})
    def delete_note(self, nid): return self._json("DELETE", f"/api/memory/notes/{nid}")

    def tts(self, text, voice):
        """Returns mp3 bytes. Raises ApiError (status 503 when voice is unavailable)."""
        return self._req("POST", "/api/tts", json={"text": text, "voice": voice}).content
