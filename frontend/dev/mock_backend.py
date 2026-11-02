"""Tiny fake backend (stdlib only) that follows docs/API.md. For frontend dev/testing.

Run:  python dev/mock_backend.py [port]      (default 8080)
Env:  MOCK_TTS=off  -> /api/tts returns 503
"""
import json
import os
import sys
from datetime import date, timedelta
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PART_ORDER = ["morning", "afternoon", "evening", "anytime"]
TODAY = date.today()
iso = lambda d: d.isoformat()

S = {
    "settings": {"assistant_name": "Sunny", "user_name": "Alex", "voice": "en-US-JennyNeural",
                 "location_name": "London", "latitude": 51.5072, "longitude": -0.1276,
                 "gemini_api_key_set": False, "timezone": "Australia/Sydney"},
    "categories": [
        {"id": 1, "name": "Chores", "color": "#A1887F"},
        {"id": 2, "name": "Exercise", "color": "#6BAA75"},
        {"id": 3, "name": "Study", "color": "#9575CD"},
        {"id": 4, "name": "Work", "color": "#5C6BC0"},
        {"id": 5, "name": "Errands", "color": "#E8B931"},
        {"id": 6, "name": "Appointments", "color": "#4FA3D1"},
        {"id": 7, "name": "Social", "color": "#D5679B"},
        {"id": 8, "name": "Self-care", "color": "#26A69A"},
        {"id": 9, "name": "Admin & Money", "color": "#78909C"},
        {"id": 10, "name": "Other", "color": "#B5ABA3"},
    ],
    "tasks": [
        {"id": 1, "title": "Morning routine", "date": None, "time": "07:30", "part_of_day": "morning",
         "category_id": 8, "notes": "", "recurrence": {"kind": "daily"},
         "checklist": [{"id": 5, "text": "Make bed"}, {"id": 6, "text": "Stretch 5 min"}, {"id": 7, "text": "Vitamins"}]},
        {"id": 2, "title": "Team stand-up", "date": None, "time": "10:00", "part_of_day": "morning",
         "category_id": 4, "notes": "", "recurrence": {"kind": "weekdays"}, "checklist": []},
        {"id": 3, "title": "Call mum", "date": iso(TODAY), "time": None, "part_of_day": "evening",
         "category_id": 7, "notes": "Ask about the trip", "recurrence": None, "checklist": []},
        {"id": 4, "title": "Water the plants", "date": iso(TODAY - timedelta(days=1)), "time": None,
         "part_of_day": "anytime", "category_id": 1, "notes": "", "recurrence": None, "checklist": []},
        {"id": 8, "title": "Buy groceries", "date": iso(TODAY), "time": "17:30", "part_of_day": "evening",
         "category_id": 5, "notes": "", "recurrence": None, "checklist": []},
        {"id": 9, "title": "Laundry", "date": None, "time": None, "part_of_day": "afternoon",
         "category_id": 1, "notes": "", "recurrence": {"kind": "weekly", "days": [5]}, "checklist": []},
    ],
    "goals": [
        {"id": 31, "title": "Pass my driving test", "horizon": "medium", "deadline": iso(date(2027, 3, 1)),
         "why": "Freedom to visit family without relying on lifts", "status": "active", "achieved_on": None, "created": iso(TODAY)},
        {"id": 32, "title": "Run a half marathon", "horizon": "medium", "deadline": None,
         "why": "", "status": "active", "achieved_on": None, "created": iso(TODAY)},
        {"id": 33, "title": "Build a year of savings", "horizon": "long", "deadline": iso(date(2029, 6, 30)),
         "why": "Feeling safe and able to say yes to the things I care about", "status": "active", "achieved_on": None, "created": iso(TODAY)},
        {"id": 34, "title": "Finish the online course", "horizon": "medium", "deadline": "2026-09-15",
         "why": "Proof to myself I can stick with something", "status": "achieved", "achieved_on": "2026-09-10", "created": "2026-05-01"},
        {"id": 35, "title": "Learn to knit", "horizon": "long", "deadline": None, "why": "", "status": "archived",
         "achieved_on": None, "created": "2026-04-01"},
    ],
    "done": {},  # (task_id, date) -> bool
    "ck_done": {},  # (task_id, date, item_id) -> bool
    "exercises": [{"id": 1, "name": "Bench Press"}, {"id": 2, "name": "Overhead Press"}, {"id": 3, "name": "Triceps Pushdown"},
                  {"id": 4, "name": "Barbell Row"}, {"id": 5, "name": "Pull-up"}, {"id": 6, "name": "Squat"}],
    "templates": [
        {"id": 1, "name": "Push A", "exercises": [
            {"exercise_id": 1, "name": "Bench Press", "sets": 3, "rep_min": 6, "rep_max": 10},
            {"exercise_id": 2, "name": "Overhead Press", "sets": 3, "rep_min": 8, "rep_max": 12},
            {"exercise_id": 3, "name": "Triceps Pushdown", "sets": 3, "rep_min": 10, "rep_max": 15}]},
        {"id": 2, "name": "Pull A", "exercises": [
            {"exercise_id": 4, "name": "Barbell Row", "sets": 3, "rep_min": 6, "rep_max": 10},
            {"exercise_id": 5, "name": "Pull-up", "sets": 3, "rep_min": 5, "rep_max": 8}]},
        {"id": 3, "name": "Legs A", "exercises": [
            {"exercise_id": 6, "name": "Squat", "sets": 4, "rep_min": 5, "rep_max": 8}]},
    ],
    "rotation": {"items": [{"template_id": 1}, {"template_id": 2}, {"template_id": 3}, {"rest": True}], "next_index": 0},
    "sessions": [],
    "memory": {"weather": {"comfortable_max": 18, "hot_above": 22, "too_hot_above": 25, "likes_rain": True, "avoid_sunburn": True},
               "notes": [{"id": 1, "text": "I work from an office most weekdays"}],
               "work": {"enabled": True, "days": [0, 1, 2, 3, 4], "start": "10:00", "end": "17:00", "holiday_region": "AU-NSW", "lunch_minutes": 60},
               "rhythm": {"wake_from": "06:00", "wake_to": "07:00", "sleep_from": "23:00", "sleep_to": "00:00"}},
    "days_off": set(),
    "next_id": 100,
}
# history: (exercise_id) -> list of {date, sets}
HIST = {1: [(iso(TODAY - timedelta(days=d)), w) for d, w in [(28, 52.5), (21, 55), (14, 57.5), (7, 60), (3, 60)]]}


KEYWORDS = [("Exercise", ["gym", "run", "workout", "walk", "yoga", "swim"]), ("Study", ["study", "revise", "course", "homework", "read"]),
            ("Work", ["meeting", "email", "stand-up", "report", "work", "client"]), ("Errands", ["buy", "groceries", "shop", "post", "collect"]),
            ("Appointments", ["dentist", "doctor", "appointment", "haircut"]), ("Social", ["call", "friend", "dinner", "party", "mum", "dad"]),
            ("Self-care", ["meditate", "bath", "sleep", "skincare", "journal"]), ("Admin & Money", ["bill", "tax", "budget", "invoice", "bank", "insurance"]),
            ("Chores", ["laundry", "clean", "dishes", "tidy", "vacuum", "plants", "bins", "wash"])]


def auto_category(t):
    text = f"{t.get('title', '')} {t.get('notes', '')}".lower()
    name = next((n for n, kws in KEYWORDS if any(k in text for k in kws)), "Other")
    return next(c["id"] for c in S["categories"] if c["name"] == name)


def nid():
    S["next_id"] += 1
    return S["next_id"]


def cat(cid):
    return next((c for c in S["categories"] if c["id"] == cid), None)


def occurs(t, d):
    r = t.get("recurrence")
    if not r:
        return t["date"] == iso(d)
    if r["kind"] == "daily":
        return True
    if r["kind"] == "weekdays":
        return d.weekday() < 5
    return d.weekday() in r.get("days", [])


def part_for(t):
    if t["time"]:
        h = int(t["time"][:2])
        return "morning" if h < 12 else "afternoon" if h < 17 else "evening"
    return t.get("part_of_day") or "anytime"


def occ(t, d, rolled=None):
    return {"task_id": t["id"], "date": iso(d), "title": t["title"], "time": t["time"],
            "part_of_day": part_for(t), "category": cat(t["category_id"]), "notes": t["notes"],
            "recurring": bool(t["recurrence"]), "rolled_over_from": rolled,
            "done": S["done"].get((t["id"], iso(d)), False),
            "checklist": [{**c, "done": S["ck_done"].get((t["id"], iso(d), c["id"]), False)} for c in t["checklist"]]}


def tmpl_plan(tid):
    t = next(x for x in S["templates"] if x["id"] == tid)
    exs = []
    for e in t["exercises"]:
        hist = HIST.get(e["exercise_id"])
        last = {"date": hist[-1][0], "sets": [{"weight": hist[-1][1], "reps": e["rep_max"]}] * e["sets"]} if hist else None
        sug = hist[-1][1] + 2.5 if hist else None
        exs.append({**e, "last": last, "suggested_weight": sug,
                    "hint": f"You hit {e['rep_max']} reps on every set last time, try {sug} kg" if sug else None})
    return {"rest": False, "template_id": tid, "name": t["name"], "exercises": exs}


def plan_at(idx):
    items = S["rotation"]["items"]
    if not items:
        return {"rest": True, "name": "Rest day"}
    it = items[idx % len(items)]
    return {"rest": True, "name": "Rest day", "template_id": None, "exercises": []} if it.get("rest") else tmpl_plan(it["template_id"])


def day(d):
    items = []
    for t in S["tasks"]:
        if t["recurrence"] is None and d == TODAY and t["date"] and t["date"] < iso(TODAY) and not S["done"].get((t["id"], t["date"])):
            t["rolled_from"] = t.get("rolled_from") or t["date"]
            t["date"] = iso(TODAY)
        if occurs(t, d):
            items.append(occ(t, d, t.get("rolled_from") if t["recurrence"] is None and d == TODAY else None))
    items.sort(key=lambda o: (o["time"] is None, o["time"] or "", PART_ORDER.index(o["part_of_day"])))
    wk = None
    if d >= TODAY:
        wk = plan_at(S["rotation"]["next_index"] + (d - TODAY).days)
    # mock public holiday: the Friday of the current week
    holiday = "Mock Public Holiday" if d == TODAY - timedelta(days=TODAY.weekday()) + timedelta(days=4) else None
    off = iso(d) in S["days_off"]
    w = S["memory"]["work"]
    work = {"start": w["start"], "end": w["end"]} if (w["enabled"] and d.weekday() in w["days"] and not holiday and not off) else None
    return {"date": iso(d), "weekday": d.strftime("%A"), "items": items, "workout": wk,
            "work": work, "holiday": holiday, "day_off": off}


MORNING_TEXT = ("Good morning, Alex! Here's how your Monday looks. It's a light rain kind of day, around fourteen at most, which suits you just fine, so lean into it and make the home bits cosy. You've got a good mix today, so let me walk you through it.\n\nStarting with life admin: you've got the groceries to pick up at half past five, and the laundry is due this afternoon. Neither is heavy, but the laundry is best started early so it's dry by tonight. Work is mostly the team stand-up at ten, and you'll want to clear your inbox right after while it's fresh. That should be the main brain-heavy stretch, and everything else is gentle by comparison.\n\nOn the exercise side, it's push day today. Bench press leads, then overhead press and triceps pushdowns. Last time you hit sixty on the bench, so there's a good chance you can nudge that up a little if the reps feel solid. I'd do it late afternoon, after the laundry is in and before the shops, so your energy is high and you're not rushing.\n\nFor the social side, you said you wanted to call your mum this evening, and she'll love hearing about the trip, so maybe save that for after dinner when you can properly settle in. There are no appointments today, which is a nice breather, though the dentist booking is still floating about for later this week.\n\nIf I were ordering the day, I'd do the morning routine, stand-up, a short email clear-out, then the laundry on. Lunch, a walk if the rain eases, then the gym, groceries on the way home, dinner, and finally the call with your mum. That leaves your evening unhurried.\n\nLooking back at yesterday, you watered the plants a bit late, but they're fine, and it counted. That kind of steady follow-through is exactly what keeps your driving test goal moving. And tomorrow looks lighter, just the stand-up and a pull session, so don't worry about squeezing everything in today. One thing at a time, and you'll be right on track. Have a lovely day!")


def briefing(kind, d):
    dd = day(d)
    done = sum(1 for i in dd["items"] if i["done"])
    nxt = next((i for i in dd["items"] if not i["done"]), None)
    texts = {"morning": MORNING_TEXT,
             "evening": "Hey you! Nice work today. Let's wind down together, there's a little left on the list but nothing that can't wait until tomorrow.",
             "weekly": "What a week! You got a good chunk of your tasks done and trained a few times. Next week, let's keep the rhythm going and be kind to ourselves."}
    return {"kind": kind, "date": iso(d), "text": texts[kind], "source": "template",
            "weather": {"summary": "Light rain", "temp_max": 14, "temp_min": 8},
            "affirmation": "You don't have to do it all today, just the next thing.",
            "stats": {"total": len(dd["items"]), "done": done, "next_up": nxt}}


def session_json(s):
    return s


def make_mp3():
    frame = bytes([0xFF, 0xFB, 0x90, 0x00]) + bytes(413)
    return frame * 80


class H(BaseHTTPRequestHandler):
    def log_message(self, fmt, *a):
        sys.stderr.write("mock: " + fmt % a + "\n")

    def send(self, code=200, body=None, ctype="application/json"):
        data = body if isinstance(body, bytes) else (b"" if body is None else json.dumps(body).encode())
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def body(self):
        n = int(self.headers.get("Content-Length") or 0)
        return json.loads(self.rfile.read(n) or b"{}") if n else {}

    def handle_any(self, method):
        from urllib.parse import urlparse, parse_qs
        u = urlparse(self.path)
        p = u.path.rstrip("/")
        q = {k: v[0] for k, v in parse_qs(u.query).items()}
        parts = p.split("/")[2:]  # after /api
        b = self.body() if method in ("POST", "PUT") else {}
        try:
            r = self.route(method, parts, q, b)
        except Exception as e:  # noqa
            import traceback; traceback.print_exc()
            return self.send(500, {"error": str(e)})
        if r is None:
            return self.send(404, {"error": f"no route {method} {p}"})
        if isinstance(r, tuple):
            return self.send(*r)
        self.send(200, r)

    do_GET = lambda s: s.handle_any("GET")
    do_POST = lambda s: s.handle_any("POST")
    do_PUT = lambda s: s.handle_any("PUT")
    do_DELETE = lambda s: s.handle_any("DELETE")

    def crud(self, key, method, parts, b, build=None):
        coll = S[key]
        if len(parts) == 1:
            if method == "GET":
                return coll
            if method == "POST":
                item = {**b, "id": nid()}
                if build: build(item)
                coll.append(item)
                return item
        else:
            i = int(parts[1])
            item = next((x for x in coll if x["id"] == i), None)
            if item is None:
                return 404, {"error": "not found"}
            if method == "GET":
                return item
            if method == "PUT":
                item.update({k: v for k, v in b.items() if k != "id"})
                if build: build(item)
                return item
            if method == "DELETE":
                coll.remove(item)
                return 204, None

    def route(self, m, parts, q, b):
        r0 = parts[0] if parts else ""
        if r0 == "settings":
            if m == "PUT":
                b = dict(b)
                if b.pop("gemini_api_key", None):
                    S["settings"]["gemini_api_key_set"] = True
                S["settings"].update(b)
            return S["settings"]
        if r0 == "categories":
            return self.crud("categories", m, parts, b)
        if r0 == "tasks":
            def build(t):
                t.setdefault("date", None); t.setdefault("time", None); t.setdefault("recurrence", None)
                t["checklist"] = [{"id": c.get("id") or nid(), "text": c["text"]} for c in t.get("checklist", [])]
                t.setdefault("notes", ""); t.setdefault("category_id", None); t.setdefault("part_of_day", "anytime")
                if t["category_id"] == "auto":
                    t["category_id"] = auto_category(t); t["category_auto"] = True
                else:
                    t["category_auto"] = False
            return self.crud("tasks", m, parts, b, build)
        if r0 == "goals":
            def gbuild(g):
                g.setdefault("deadline", None); g.setdefault("why", ""); g.setdefault("status", "active")
                g.setdefault("created", iso(TODAY)); g.setdefault("achieved_on", None)
            res = self.crud("goals", m, parts, b, gbuild)
            if m == "GET" and len(parts) == 1:
                res = sorted(res, key=lambda g: g["status"] != "active")
            return res
        if r0 == "days":
            start = date.fromisoformat(q["start"]); n = int(q.get("days", 7))
            return [day(start + timedelta(days=i)) for i in range(n)]
        if r0 == "occurrences":
            d = b["date"]; tid = b["task_id"]
            t = next(x for x in S["tasks"] if x["id"] == tid)
            if parts[1] == "toggle":
                S["done"][(tid, d)] = b["done"]
            else:
                S["ck_done"][(tid, d, b["item_id"])] = b["done"]
            return occ(t, date.fromisoformat(d))
        if r0 == "exercises":
            if len(parts) == 3:
                return [{"date": d, "top_weight": w, "best_reps": 8, "est_1rm": round(w * 1.3, 1)} for d, w in HIST.get(int(parts[1]), [])]
            return self.crud("exercises", m, parts, b)
        if r0 == "templates":
            def build(t):
                for e in t.get("exercises", []):
                    ex = next((x for x in S["exercises"] if x["id"] == e["exercise_id"]), None)
                    e["name"] = ex["name"] if ex else "?"
            return self.crud("templates", m, parts, b, build)
        if r0 == "rotation":
            if m == "PUT":
                S["rotation"]["items"] = b["items"]
                S["rotation"]["next_index"] = min(S["rotation"]["next_index"], max(len(b["items"]) - 1, 0))
            return S["rotation"]
        if r0 == "gym":
            if parts[1] == "next":
                return plan_at(S["rotation"]["next_index"])
            if parts[1] == "rest":
                S["rotation"]["next_index"] = (S["rotation"]["next_index"] + 1) % len(S["rotation"]["items"])
                return S["rotation"]
        if r0 == "sessions":
            if len(parts) == 1 and m == "POST":
                ex = next((s for s in S["sessions"] if not s["completed"]), None)
                if ex: return ex
                plan = tmpl_plan(b["template_id"])
                sets = []
                for e in plan["exercises"]:
                    for n in range(e["sets"]):
                        sets.append({"exercise_id": e["exercise_id"], "set_no": n + 1, "weight": e["suggested_weight"] or 20,
                                     "reps": e["rep_max"], "done": False})
                s = {"id": nid(), "template_id": b["template_id"], "name": plan["name"], "date": iso(TODAY), "completed": False, "sets": sets}
                S["sessions"].append(s)
                return s
            s = next(x for x in S["sessions"] if x["id"] == int(parts[1]))
            if parts[2] == "sets":
                s["sets"] = b["sets"]
                return s
            if parts[2] == "complete":
                s["completed"] = True
                S["rotation"]["next_index"] = (S["rotation"]["next_index"] + 1) % len(S["rotation"]["items"])
                return s
        if r0 == "briefing":
            return briefing(q.get("kind", "morning"), date.fromisoformat(q["date"]) if q.get("date") else TODAY)
        if r0 == "day-note":
            d = date.fromisoformat(q["date"]) if q.get("date") else TODAY
            n = len(day(d)["items"])
            return {"date": iso(d), "source": "template",
                    "text": f"{n} things on today, with room to breathe in between. Start with the one that matters most and the rest will follow."}
        if r0 == "digest":
            import time
            time.sleep(float(os.getenv("MOCK_DIGEST_DELAY", "1.5")))
            d = date.fromisoformat(q["date"]) if q.get("date") else TODAY
            if os.getenv("MOCK_DIGEST") == "fail":
                return 500, {"error": "digest failed"}
            wx = None if os.getenv("MOCK_WEATHER") == "off" else {
                "summary": "Light rain", "temp_max": 14, "temp_min": 8, "precip_prob": 80, "uv_max": 3.5}
            return {"date": iso(d), "weather": wx, "source": "template",
                    "weather_comment": None if wx is None else "A proper rainy one, 14 at most. Cosy if you're home, and nice to listen to from your desk. Pack sun cream anyway if the clouds break.",
                    "overview": "Your morning is about getting settled: the routine, then stand-up at ten. The afternoon is wide open, which is a lovely chance to get the groceries sorted without rushing. Call your mum in the evening, and keep a breather before push day. All of it nudges you towards that driving test."}
        if r0 == "days-off":
            if m == "POST":
                S["days_off"].add(b["date"])
            elif m == "DELETE":
                S["days_off"].discard(parts[1])
                return 204, None
            return sorted(S["days_off"])
        if r0 == "memory":
            mem = S["memory"]
            if len(parts) == 1:
                return mem
            if parts[1] == "rhythm" and m == "PUT":
                mem["rhythm"].update(b)
                return mem
            if parts[1] == "work" and m == "PUT":
                mem["work"].update(b)
                return mem
            if parts[1] == "weather" and m == "PUT":
                mem["weather"].update(b)
                return mem
            if parts[1] == "notes":
                if m == "POST":
                    mem["notes"].append({"id": nid(), "text": b["text"]})
                else:
                    i = int(parts[2])
                    if m == "PUT":
                        for n in mem["notes"]:
                            if n["id"] == i:
                                n["text"] = b["text"]
                    elif m == "DELETE":
                        mem["notes"] = [n for n in mem["notes"] if n["id"] != i]
                return mem
        if r0 == "tts":
            if os.getenv("MOCK_TTS") == "off":
                return 503, {"error": "tts unavailable"}
            return 200, make_mp3(), "audio/mpeg"
        return None


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
    print(f"mock backend on :{port}")
    ThreadingHTTPServer(("0.0.0.0", port), H).serve_forever()
