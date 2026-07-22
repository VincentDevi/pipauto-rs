# Calendar HTML and CSS implementation plan

## Status and sources

This document records the implemented presentation architecture for the calendar described in
[`documentations/CALENDAR_PRD.md`](../documentations/CALENDAR_PRD.md). The registered route,
templates, styles, and bounded backend query now exist; follow-up interaction remains unimplemented.

The current implemented browser surface remains documented in [`docs/frontend.md`](frontend.md).
The calendar follows that guide: Rust and Tera own authoritative HTML,
ordinary navigation works without JavaScript, HTMX is optional progressive enhancement, and
`assets/static/css/app.css` remains the first-release stylesheet.

The earlier [calendar and drag-and-drop research](calendar-drag-drop-options.md) remains useful for
future interaction choices. Dragging, resizing, and slot-based creation are outside this MVP and do
not influence the HTML contract below.

## Product boundary

The MVP calendar is a read-only projection of interventions:

- Month and Week are the only views; Month is the default.
- The configured workshop timezone determines the current date and visible range. Its initial value
  is `Europe/Brussels`, and weeks run Monday through Sunday.
- Every intervention has a required UTC start instant and estimated duration from 30 minutes through
  24 hours in 30-minute increments. Browser forms collect workshop-local date and time separately.
- Draft and Completed interventions appear. Cancelled interventions do not.
- Entries show start time, estimated duration, captured customer name, captured registration and
  vehicle make/model, and textual status.
- Estimated duration controls Week height. Overlaps display side by side, and work crossing midnight
  is represented on every affected date.
- **New intervention** starts the existing active-vehicle-first creation flow.
- Entries and calendar slots are informational in the MVP. They are not links, buttons, drag
  handles, or implicit creation controls.

Clickable Draft/Completed entries and date- or slot-prefilled creation are follow-up behaviors.
Generic events, Day/agenda views, recurrence, resources, drag-and-drop, and resizing remain out of
scope.

## Chosen rendering approach

### Server-owned HTML

The implementation uses a full page and one replaceable calendar fragment:

```text
assets/views/pages/calendar.html
  extends layouts/base.html
  includes fragments/calendar.html

assets/views/fragments/calendar.html
  #calendar-region
    calendar page header and New intervention action
    view and period navigation
    Month or Week representation
    bounded loading/error/empty state
```

`GET /calendar?view=month|week&date=YYYY-MM-DD` is the reproducible public address. Missing values
default to Month and the current workshop-local date. Invalid values return a calendar-owned
validation state rather than silently choosing a different period.

Previous, Today, Next, Month, Week, day-selection, and overflow controls are real GET links. They
may add `hx-get`, `hx-target="#calendar-region"`, `hx-swap="outerHTML"`, and `hx-push-url="true"`.
The normal `href` remains authoritative and renders the complete page when HTMX or JavaScript is
unavailable.

No client calendar library is required for the informational MVP. The server already has to
calculate workshop-local periods, overlaps, captured identity, and midnight segments correctly. A
client-rendered calendar would duplicate those decisions, introduce a separate JSON presentation
boundary, and add JavaScript/CSS that is not needed for navigation or read-only display.

### Semantic structure

Use headings, navigation landmarks, ordered/unordered lists, `article`, and `time`. Do not add
`role="grid"`: an ARIA grid would require spreadsheet-style keyboard behavior that the MVP does not
need. Visual CSS Grid placement must not replace meaningful document order.

A simplified fragment shape is:

```html
<div id="calendar-region" class="calendar-page" data-calendar-view="month">
  <header class="calendar-header page-header">
    <div>
      <p class="eyebrow">Workshop schedule</p>
      <h1 id="calendar-heading">Calendar</h1>
      <p class="summary">July 2026 · Europe/Brussels</p>
    </div>
    <a class="button" href="/vehicles">New intervention</a>
  </header>

  <div class="calendar-toolbar" aria-label="Calendar controls">
    <nav class="calendar-period-navigation" aria-label="Calendar period">
      <!-- Real Previous, Today, and Next links. -->
    </nav>
    <div class="calendar-period-heading"><!-- Period and timezone labels. --></div>
    <nav class="calendar-view-navigation" aria-label="Calendar view">
      <!-- Real Month and Week links. -->
    </nav>
  </div>

  <!-- Include exactly one Month or Week view for the requested representation. -->
</div>
```

The vehicle-selection address is `/vehicles`, the existing vehicle-first workflow used by the
dashboard and intervention list.

## Presentation model

Controllers pass presentation-safe values rather than persistence rows. IDs and local URLs are
opaque server-built strings. Dates, labels, and numeric layout values are fully validated before
template rendering.

### Calendar page

| Value | Purpose |
| --- | --- |
| `view` | Validated `month` or `week`; controls the selected view link. |
| `period_label` | Human-readable workshop-local month or week range. |
| `timezone_label` | Display-safe configured timezone name. |
| `previous_href`, `today_href`, `next_href` | Reproducible navigation URLs for the active view. |
| `month_href`, `week_href` | View-switch URLs retaining the selected workshop-local date. |
| `selected_date` | Workshop-local day used by focused narrow layouts. |
| `new_intervention_href` | Existing active-vehicle selection entry point. |
| `days` | Monday-first days needed by the selected period. |
| `has_interventions` | Distinguishes a genuinely empty period from a rendering/query failure. |

### Day and entry

Each day supplies its ISO date, short and long labels, in-period/today/selected flags, normal GET
selection URL, entry count, and display segments. A display segment supplies:

- start label and machine-readable local/instant value for a `time` element;
- estimated-duration label;
- captured customer display name;
- captured registration and vehicle make/model;
- textual Draft or Completed status and its approved style variant;
- continuation-before and continuation-after labels;
- stable chronological position within its day;
- validated Week layout integers where applicable; and
- a non-interactive identity suitable for an `article` accessible name.

The server splits a midnight-crossing intervention into one segment per affected workshop-local
date. Every segment repeats enough identity and status information to remain understandable and
announces **Continues from previous day** and/or **Continues next day** in text. The underlying
intervention is not duplicated or changed.

Customer name and vehicle registration/make/model come from immutable intervention snapshots. The
calendar never substitutes live customer or vehicle display values after creation.

### Week geometry

Use 48 half-hour rows per day. Before rendering, the server:

1. Converts each timed intervention into workshop-local daily segments.
2. Sorts segments deterministically by start, end, lifecycle, and opaque identifier.
3. Assigns the lowest free overlap lane.
4. Records the maximum simultaneous lane count for the segment's overlap group.
5. Produces integer `start_minute`, `span_minutes`, `lane`, and `lane_count` values.

`start_minute` is in the inclusive range 0–1,439. `span_minutes` is positive and clipped only at
the day boundary because continuation is rendered separately. The visual background retains 48
labelled half-hour rows, while minute geometry represents valid starts such as 09:15 without
rounding. `lane` is zero-based and smaller than the positive `lane_count`. Templates must never
interpolate unvalidated customer or domain text into a `style` attribute.

On a daylight-saving transition day, visible start/end labels and elapsed duration remain
authoritative. The presentation does not invent or duplicate interactive rows for a missing or
repeated wall time; entries remain informational and accessible text explains their actual elapsed
duration.

The intended visual hook is numeric CSS custom properties:

```html
<article class="calendar-entry calendar-entry--draft week-entry"
  style="--start-minute: 540; --span-minutes: 120; --lane: 0; --lane-count: 2;">
  <!-- Visible time, vehicle/customer identity, and Draft text. -->
</article>
```

If the application later adopts a Content Security Policy that disallows inline styles, replace
this hook with server-selected finite utility classes or positioned wrappers without weakening the
validation contract.

## Month HTML and CSS

### Wide layout

Render weekday labels followed by a Monday-first semantic list of day sections. CSS turns only the
visual container into seven equal columns:

```html
<ol class="month-days">
  <li class="month-day">
    <section aria-labelledby="day-2026-07-20">
      <h2 id="day-2026-07-20"><time datetime="2026-07-20">Monday 20 July</time></h2>
      <ol class="month-entry-list"><!-- calendar-entry articles --></ol>
    </section>
  </li>
</ol>
```

Wide day cells display a bounded initial group of entries. When more entries exist, a native
`details.calendar-overflow` exposes every remaining entry in the same response. Its summary states
the exact hidden count, such as **Show 4 more interventions**. CSS must not use line clamping,
fixed-height clipping, or an unlabeled `+4` as the only disclosure.

Month continuation segments include visible **From previous day** or **Continues next day** text.
Directional decoration may support the text but cannot replace it.

### Phone layout

Below `42rem`, the month date matrix reaches the calendar content edges so seven date targets retain
the shared minimum target size without causing page-level horizontal scrolling. Non-selected days
show their date, intervention count, and textual/visually-hidden status summary. The selected day's
section spans all seven columns and exposes its complete entry list.

Every date target is a normal URL using the Month view and that date. The server marks exactly one
day selected. In the current month it defaults to today; outside the current month it defaults to
the requested date, clamped to the visible month only when Previous/Next cannot preserve that day.

## Week HTML and CSS

### Wide layout

At `64rem` and above, show the complete Monday–Sunday week over a 24-hour time axis. Time labels and
day columns share the same half-hour row sizing. The calendar surface may scroll vertically inside
the page, but all 24 hours must exist in the HTML and be reachable by keyboard and scrolling.

Each day is a labelled section. Timed entries remain in chronological DOM order and use the
validated custom properties for vertical span and horizontal overlap placement. CSS calculates the
lane width and offset from `--lane` and `--lane-count`; it does not decide conflicts or dates.

Midnight continuations use separate daily segments and remain associated through their visible
captured identity and continuation text.

### Phone and narrow-tablet layout

Below `64rem`, render a seven-item weekday selector with each day's count, followed by only the
selected day's complete 24-hour timeline. The same day sections are used; CSS hides non-selected
visual timelines in the focused layout, while normal GET links make every day reachable without
JavaScript. The selected date, weekday, and week range appear in visible headings.

Overlapping entries remain side by side when labels fit. At the narrowest supported width, lanes may
stack as consecutive cards within the same half-hour region, with explicit start/end text, rather
than shrinking text below a usable size. This is a presentation fallback only and does not alter
chronology or duration.

## CSS contract

Calendar rules live in `assets/static/css/app.css` and reuse existing tokens for
color, typography, spacing, borders, radius, focus, minimum target size, and the shell. New literal
colors or a second stylesheet are not needed.

Implemented class families:

| Area | Classes |
| --- | --- |
| Root/header | `.calendar-page`, `.calendar-header`, `.calendar-toolbar`, `.calendar-period-navigation`, `.calendar-view-navigation` |
| Shared entry | `.calendar-entry`, `.calendar-entry--completed`, `.calendar-entry-status`, `.calendar-entry-continuation` |
| Month | `.calendar-month-grid`, `.calendar-day`, `.calendar-date-selector`, `.calendar-overflow` |
| Week | `.calendar-week-grid`, `.calendar-week-time-axis`, `.calendar-week-entry`, `.calendar-week-selector` |
| Responsive | `.calendar-wide`, `.calendar-focused`, `.calendar-focused-timeline` |
| State | `.calendar-loading`, `.calendar-empty`, `.calendar-selected-empty` |

The implementation follows this layout shape and is verified against rendered browser fixtures:

```css
.calendar-month-grid {
  display: grid;
  grid-template-columns: repeat(7, minmax(0, 1fr));
}

.calendar-week-grid {
  --calendar-minute-height: 0.1rem;
  display: grid;
  grid-template-columns: 4.5rem repeat(7, minmax(0, 1fr));
}

.calendar-week-entry-position {
  position: absolute;
  top: calc(var(--calendar-start) * var(--calendar-minute-height));
  left: calc((100% / var(--calendar-lanes)) * var(--calendar-lane));
  width: calc(100% / var(--calendar-lanes));
  min-height: max(calc(var(--calendar-span) * var(--calendar-minute-height)), var(--target-size));
}
```

Production rules must add token-based spacing, borders, focus treatment, overlap gaps, and content
overflow behavior. A segment may grow beyond its calculated minimum height when text zoom requires
it; the narrow-layout fallback then stacks conflicting cards within the labelled time region. The
layout must never clip identity or status merely to preserve geometric height.

Draft and Completed styles use existing semantic surface/border tokens and always retain visible
status text. Today and selected date use separate borders or labels so they are distinguishable
without relying on color. Entry text may wrap; essential customer, vehicle, time, status, and
continuation information must not be truncated away.

Breakpoints follow the existing stylesheet: below `64rem`, Month uses the seven-date selector plus
focused selected day and Week uses day selectors plus one focused timeline; at `64rem` and above,
the sidebar and full seven-day Month/Week layouts appear.

At 200% text zoom, switch to the focused layout whenever the remaining CSS-pixel width crosses the
same breakpoint. Do not preserve a dense desktop grid by creating page-level horizontal scrolling.

## Accessibility and progressive enhancement

- Keep source order chronological and headings descriptive; visual placement alone must not carry
  meaning.
- Provide a skip link through the existing shell and an additional link to the selected day or time
  axis when useful.
- Give every navigation link and entry an accessible name that includes its date/time and vehicle.
- Use `time` with machine-readable values, while displaying workshop-local labels.
- Preserve visible focus rings and the shared 44px minimum target size.
- Mark the active view with `aria-current="page"` and today with understandable visible text; do not
  overload `aria-current` for multiple concepts on one control.
- During an HTMX request, keep the previous calendar visible, mark `#calendar-region` busy, and show
  the existing loading indicator. After replacement, follow the frontend guide's focus recovery.
- Respect reduced-motion rules; the calendar needs no motion to communicate position or status.

## States and recovery

| State | Required presentation |
| --- | --- |
| Empty period | Keep navigation and New intervention visible; state that no Draft or Completed interventions fall in the period. |
| Loading | Retain existing entries, set bounded busy state, and announce loading politely. |
| Invalid query | Explain the invalid view/date and link to the current Month without changing records. |
| Unavailable | Keep Calendar navigation active, state that records are unchanged, show correlation reference when available, and provide Retry. |
| Unexpected error | Use the shared safe error contract; expose no infrastructure or persistence detail. |
| Expired session | Follow the safe login redirect with the local calendar URL as the return path. |

There is no calendar mutation in the MVP, so no calendar-specific `409` or CSRF form is needed.
Intervention creation retains its own validation, chronology, lifecycle, and CSRF behavior.

## Implemented sequence

1. Delivered the time-aware intervention domain/query changes required by the PRD, including
   mandatory duration and captured customer/vehicle presentation values.
2. Added the bounded overlap query and presentation model that computes workshop-local days, midnight
   segments, and overlap lanes.
3. Registered the authenticated GET route and added Calendar to the shell's active navigation.
4. Added the thin page, replaceable fragment, Month markup, and focused phone behavior.
5. Added Week markup and server-calculated layout custom properties.
6. Added calendar CSS using existing tokens and breakpoints, then optional HTMX navigation.
7. Updated `docs/frontend.md` with the implemented route and component contracts.

## Verification plan

- Unit-test workshop-local Month/Week boundaries, Monday-first weeks, leap days, DST transitions,
  midnight splits, overlap lanes, and deterministic segment ordering.
- Render-test escaped customer/vehicle values, duration and status text, continuation labels,
  overflow disclosures, and validated layout values.
- Request-test authentication, `no-store`, defaults, invalid queries, Today/Previous/Next URLs,
  HTMX/full-page parity, and inclusion/exclusion/overlap rules.
- Browser-test desktop, tablet, phone, and JavaScript-disabled projects for both views, all 24 Week
  hours, busy days, focused-day navigation, 200% zoom, keyboard focus, and no page overflow.
- Run Axe on populated Month, populated Week, empty, invalid-query, and unavailable examples.
- Confirm no MVP control implies clicking entries, clicking slots, dragging, resizing, recurrence,
  resources, generic events, or Cancelled intervention visibility.
