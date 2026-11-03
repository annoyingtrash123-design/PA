// Shapes of the State and the REST-like API (see docs/API.md and docs/WEB_ARCH.md).

export type PartOfDay = "morning" | "afternoon" | "evening" | "anytime";

export interface Settings {
  assistant_name: string;
  user_name: string;
  voice: string;
  location_name: string;
  latitude: number;
  longitude: number;
  timezone: string;
  gemini_api_key_set?: boolean;
}

export interface Category {
  id: number;
  name: string;
  color: string;
}

export type Recurrence = null | { kind: "daily" } | { kind: "weekdays" } | { kind: "weekly"; days: number[] };

export interface ChecklistItem {
  id?: number;
  text: string;
}

export interface Task {
  id: number;
  title: string;
  date: string | null;
  time: string | null;
  part_of_day: PartOfDay | null;
  category_id: number | null | "auto";
  category_auto?: boolean;
  notes: string;
  recurrence: Recurrence;
  checklist: ChecklistItem[];
}

export interface Occurrence {
  task_id: number;
  date: string;
  title: string;
  time: string | null;
  part_of_day: PartOfDay | null;
  category: Category | null;
  notes: string;
  recurring: boolean;
  rolled_over_from: string | null;
  done: boolean;
  checklist: { id: number; text: string; done: boolean }[];
}

export interface WorkoutExercise {
  exercise_id: number;
  name: string;
  sets: number;
  rep_min: number;
  rep_max: number;
  last?: { date: string; sets: { weight: number; reps: number }[] } | null;
  suggested_weight?: number | null;
  hint?: string | null;
  assisted?: boolean;
  start_weight?: number | null;
  note?: string;
}

export interface WorkoutPlan {
  rest: boolean;
  template_id?: number;
  name?: string;
  kind?: TemplateKind;
  note?: string;
  exercises?: WorkoutExercise[];
  options?: string[];
  minutes_min?: number | null;
  minutes_max?: number | null;
}

export interface Day {
  date: string;
  weekday: string;
  items: Occurrence[];
  workout: WorkoutPlan | null;
  work: { start: string; end: string } | null;
  holiday: string | null;
  day_off: boolean;
}

export interface Goal {
  id: number;
  title: string;
  horizon: "medium" | "long";
  deadline: string | null;
  why: string;
  status: "active" | "achieved" | "archived";
  achieved_on: string | null;
  created: string;
}

export interface Memory {
  weather: { comfortable_max: number; hot_above: number; too_hot_above: number; likes_rain: boolean; avoid_sunburn: boolean };
  work: { enabled: boolean; days: number[]; start: string; end: string; holiday_region: string; lunch_minutes: number };
  rhythm: { wake_from: string; wake_to: string; sleep_from: string; sleep_to: string };
  notes: { id: number; text: string }[];
}

export type TemplateKind = "strength" | "cardio";

export interface Exercise {
  id: number;
  name: string;
  /** Counterweighted / assisted machine: a higher weight is easier. */
  assisted?: boolean;
}
export interface TemplateExercise {
  exercise_id: number;
  name?: string;
  sets: number;
  rep_min: number;
  rep_max: number;
  start_weight?: number | null;
  note?: string;
  /** Write-only convenience for the template body: sets the shared exercise flag. */
  assisted?: boolean;
}
export interface Template {
  id: number;
  name: string;
  kind?: TemplateKind;
  note?: string;
  exercises: TemplateExercise[];
  options?: string[];
  minutes_min?: number | null;
  minutes_max?: number | null;
}
export interface Rotation {
  items: { template_id?: number; rest?: boolean }[];
  next_index: number;
}
export interface SessionSet {
  exercise_id: number;
  set_no: number;
  weight: number;
  reps: number;
  done: boolean;
}
export interface Session {
  id: number;
  template_id: number;
  name: string;
  date: string;
  completed: boolean;
  sets: SessionSet[];
  cardio?: { activity: string; minutes: number } | null;
}
export interface HistoryPoint {
  date: string;
  top_weight: number;
  best_reps?: number;
  est_1rm?: number | null;
}

export interface Weather {
  summary: string;
  temp_max: number;
  temp_min: number;
  precip_prob: number;
  uv_max: number;
}

export interface Holiday {
  date: string;
  name: string;
}

export interface State {
  version: number;
  next_id: number;
  settings: Settings;
  categories: Category[];
  tasks: Task[];
  completions: Record<string, boolean>;
  checklist_done: Record<string, boolean>;
  rolled_over: Record<string, string>;
  missed?: Record<string, string>;
  days_off: string[];
  goals: Goal[];
  memory: Memory;
  gym: { exercises: Exercise[]; templates: Template[]; rotation: Rotation; sessions: Session[]; next_since?: string | null };
}

/** The morning briefing as four spoken parts. */
export interface MorningParts {
  greeting: string;
  weather_comment: string | null;
  affirmation: string;
  briefing: string;
}

export interface BriefingPrep {
  kind: string;
  date: string;
  weather: Weather | null;
  stats: { total?: number; done?: number; gym_sessions?: number; [k: string]: unknown };
  affirmation?: string;
  template_text: string;
  /** morning only: the no-AI text in the same four parts (template_text = these joined) */
  template_parts?: MorningParts | null;
  cache_key: string;
  /** hash of the plan the text is about (no weather); changes when plans change */
  plan_fingerprint?: string;
  system_prompt: string;
  user_prompt: string;
  max_words?: number;
}

export interface DigestPrep {
  date: string;
  weather: Weather | null;
  cache_key: string;
  system_prompt: string;
  user_prompt: string;
  template: { weather_comment: string | null; overview: string };
}

export interface CoreResponse {
  status: number;
  body: unknown;
  state: State | null;
}
