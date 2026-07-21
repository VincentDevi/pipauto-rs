# Calendar

## Product boundary

Calendar is an authenticated, read-only projection of interventions. It is not a separate
appointment or generic event system. The MVP displays Month and Week views and starts new work
through the existing active-vehicle-first intervention workflow.

Every intervention has a required start and estimated duration. Entries display the customer and
vehicle identity captured at creation so later record edits do not rewrite historical presentation.

Draft and Completed entries are informational in this milestone. Selecting entries or slots,
dragging, resizing, and prefilled slot creation are follow-up behavior and must not appear as active
controls in these wireframes.

## Calendar page

| Property | Specification |
| --- | --- |
| Route | `GET /calendar?view=month|week&date=YYYY-MM-DD` |
| Access | Authenticated |
| Defaults | Month and the current date in the configured workshop timezone |
| Entry/exit | Desktop/phone Calendar navigation; exits through New intervention or another primary destination |
| Query | Validated Month/Week view and workshop-local anchor date; weeks start Monday |
| Data | All overlapping Draft and Completed interventions in the visible range; no pagination; Cancelled excluded |
| Entry content | Start time, duration, captured customer and vehicle identity, textual status, continuation where applicable |
| Primary action | New intervention; select an active vehicle before opening its form |
| Navigation | Previous, Today, Next, Month, Week, and focused-day links are normal GET navigation |
| Backend | Bounded overlap query, immutable customer/vehicle snapshots, workshop timezone, duration, midnight segments, and overlap layout |

The route inventory includes the authenticated Calendar read path and responsive Month and Week
views. Week presents all seven days together on wide screens and a normal-GET focused day on
narrow screens, with every half-hour row reachable in either layout.

Calendar navigation may replace the calendar region with HTMX and update browser history. Every
control retains a complete `href`, so refresh, Back, copied URLs, and JavaScript-disabled use
reproduce the same view.

## Month view

Month uses Monday-first day sections. Draft and Completed status is always written on entries and
also receives distinct styling. Days outside the selected month remain visibly muted when needed to
complete the first and last week.

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ PIPAUTO           │ Calendar                                          [ + New intervention ]│
│   Dashboard       ├──────────────────────────────────────────────────────────────────────────┤
│   Customers       │ [Previous] [Today] [Next]     July 2026     [ Month ] [Week]             │
│   Vehicles        │ Timezone: Europe/Brussels                                              │
│   Interventions   │                                                                          │
│ ▌ Calendar        │ Mon          Tue          Wed          Thu          Fri          Sat  Sun │
│   Knowledge       │ ┌───────────┬────────────┬────────────┬────────────┬────────────┬────┬───┐│
│   Invoices        │ │ 29 Jun    │ 30 Jun     │ 1          │ 2          │ 3          │ 4  │ 5 ││
│                   │ ├───────────┼────────────┼────────────┼────────────┼────────────┼────┼───┤│
│                   │ │ 6         │ 7          │ 8          │ 9          │ 10         │ 11 │12 ││
│                   │ │09:00      │            │13:30       │            │08:00       │    │   ││
│                   │ │1-ABC-234  │            │2-DEF-567  │            │4-JKL-901  │    │   ││
│                   │ │VW Golf    │            │Fiat Panda │            │Ford Transit│    │   ││
│                   │ │Rossi      │            │Bianchi    │            │Conti       │    │   ││
│                   │ │DRAFT      │            │COMPLETED  │            │DRAFT       │    │   ││
│                   │ ├───────────┼────────────┼────────────┼────────────┼────────────┼────┼───┤│
│                   │ │ 13        │ 14         │ 15         │ 16         │ 17         │ 18 │19 ││
│                   │ │11:30 · 3-GHI-890 · Opel Corsa · Verdi · COMPLETED                   ││
│                   │ │15:00 · 1-ABC-234 · Rossi · DRAFT · 3 h → continues next day         ││
│                   │ │                         │[Show 3 more interventions ▾]                ││
│                   │ └───────────┴────────────┴────────────┴────────────┴────────────┴────┴───┘│
│                   │                                                                          │
│ Filippo           │ Entries are informational in this release.                              │
│ Sign out          │                                                                          │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

Every affected date receives a continuation segment. A segment says **From previous day** or
**Continues next day**; an arrow may support but never replace that text. Busy days show an exact
native disclosure such as **Show 3 more interventions**, containing all entries in the initial
response.

### Phone wireframe

```text
┌──────────────────────────────┐
│ PIPAUTO             Calendar │
├──────────────────────────────┤
│ Calendar                     │
│ [ + New intervention       ] │
│ [Prev] [Today] [Next]        │
│ July 2026   [Month] [Week]   │
│ Europe/Brussels              │
│                              │
│ Mo  Tu  We  Th  Fr  Sa  Su   │
│ 29  30   1   2   3   4   5  │
│  6   7   8   9  10  11  12  │
│ 13  14 [15] 16  17  18  19  │
│ 20  21  22  23  24  25  26  │
│ 27  28  29  30  31   1   2  │
│             •2               │
│                              │
│ Wednesday 15 July · 2 jobs   │
│ ┌──────────────────────────┐ │
│ │ 09:00 · 2 h · DRAFT      │ │
│ │ Rossi                    │ │
│ │ 1-ABC-234 · VW Golf      │ │
│ └──────────────────────────┘ │
│ ┌──────────────────────────┐ │
│ │ 11:30 · 1 h · COMPLETED  │ │
│ │ Verdi                    │ │
│ │ 3-GHI-890 · Opel Corsa   │ │
│ └──────────────────────────┘ │
├──────────────────────────────┤
│Home Vehicles Calendar Jobs More│
└──────────────────────────────┘
```

The date matrix reaches the calendar content edges at the narrowest width so every date remains a
usable target without page-level horizontal scrolling. A selected date is visibly labelled in
addition to its styling. Date links reload Month with that date and expand its complete list below
the matrix. Counts never replace the selected day's full entry content.

## Week view

Week displays the complete Monday–Sunday period and all 24 hours. Entries use estimated duration
for height. Overlapping entries share horizontal space; their order and lanes are server-calculated.

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ PIPAUTO           │ Calendar                                          [ + New intervention ]│
│   Dashboard       ├──────────────────────────────────────────────────────────────────────────┤
│   Customers       │ [Previous] [Today] [Next]  13–19 July 2026  [Month] [ Week ]             │
│   Vehicles        │ Europe/Brussels                                                   │
│   Interventions   │             Mon 13   Tue 14   Wed 15   Thu 16   Fri 17   Sat 18  Sun 19 │
│ ▌ Calendar        │─────────────┼──────────┼──────────┼──────────┼──────────┼────────┼────────│
│   Knowledge       │             │          │          │          │          │        │        │
│   Invoices        │ 00:00       │          │          │          │          │        │        │
│                   │ 01:00       │          │          │          │          │        │        │
│                   │    ⋮        │          │          │          │          │        │        │
│                   │ 08:00       │┌────────┐│          │          │┌───────┐ │        │        │
│                   │ 09:00       ││09:00   ││          │┌────┐┌──┐││08:30  │ │        │        │
│                   │ 10:00       ││Golf    ││          ││Golf││Fi│││Transit│ │        │        │
│                   │ 11:00       ││Rossi   ││          ││DRFT││CO│││DRAFT  │ │        │        │
│                   │ 12:00       ││DRAFT   ││          │└────┘└──┘│└───────┘ │        │        │
│                   │    ⋮        │└────────┘│          │ overlaps │          │        │        │
│                   │ 23:00       │          │          │          │→ continues Saturday      │
│                   │ 24:00       │          │          │          │          │        │        │
│                   │             Complete 24-hour axis; scroll within this region              │
│ Filippo           │                                                                          │
│ Sign out          │                                                                          │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

The internal time surface may scroll vertically, but every hour exists and remains reachable.
Overlapping cards show full start, identity, and status information through wrapping or an
accessible label. Duration and placement never imply recorded labour time.

### Phone wireframe

```text
┌──────────────────────────────┐
│ PIPAUTO             Calendar │
├──────────────────────────────┤
│ Calendar                     │
│ [ + New intervention       ] │
│ [Prev] [Today] [Next]        │
│ 13–19 Jul 2026 [Month] [Week]│
│ Europe/Brussels              │
│                              │
│ [M13] T14 [W15·2] T16 F17 S18 S19│
│ Wednesday 15 July            │
│                              │
│ 00:00 ─────────────────────  │
│ 01:00 ─────────────────────  │
│   ⋮                          │
│ 09:00 ┌────────┐┌─────────┐  │
│       │Golf    ││Fiat     │  │
│ 10:00 │Rossi   ││Bianchi  │  │
│       │DRAFT   ││COMPLETED│  │
│ 11:00 └────────┘└─────────┘  │
│   ⋮                          │
│ 23:00 ─────────────────────  │
│ 24:00 ─────────────────────  │
├──────────────────────────────┤
│Home Vehicles Calendar Jobs More│
└──────────────────────────────┘
```

Phone and narrow-tablet Week use day selectors with counts and one complete selected-day timeline.
The other six days remain normal GET links. At the narrowest width, overlaps may become consecutive
cards in the same time region rather than reducing text below a usable size.

## Tablet behavior

- At widths where seven Month columns retain readable content, Month uses the wide grid; otherwise
  it uses the focused selected-day treatment.
- Week remains focused on one selected day until the desktop sidebar breakpoint provides enough
  width for seven time columns.
- No layout introduces page-level horizontal scrolling. The complete 24-hour Week surface may use a
  bounded vertical scroller.
- Browser zoom naturally selects the focused layout when fewer CSS pixels remain.

## Entry content and status

Each entry displays:

1. Workshop-local start time.
2. Estimated duration.
3. Captured customer name.
4. Captured vehicle registration and make/model.
5. Visible **Draft** or **Completed** status.
6. Visible continuation text when the intervention crosses a day boundary.

Draft and Completed entries use different border/surface treatments, but status is never conveyed
by color alone. Cancelled interventions never appear. MVP entries are `article` content, not links;
the cursor and focus treatment must not imply that they open another page.

## States and error coverage

### Empty period

```text
┌────────────────────────────────────────────┐
│ July 2026                  [Month] [Week]  │
│ No scheduled interventions this month.    │
│ Draft and Completed work will appear here.│
│ [ + New intervention ]                    │
└────────────────────────────────────────────┘
```

Navigation remains usable and **New intervention** continues to open the vehicle-first workflow.

### Loading and unavailable

- Initial navigation is server-rendered. During HTMX navigation, keep the current calendar visible,
  mark only the calendar region busy, and announce **Loading calendar…**.
- An invalid view/date explains the problem and offers **Open current month**.
- A `503` or unexpected error keeps Calendar selected, says records are unchanged, shows a safe
  reference when supplied, and offers **Retry**.
- Expired sessions use the existing safe login redirect and retain the local calendar return path.

## Follow-up interaction boundary

Only a later approved milestone may make:

- Draft entries open edit;
- Completed entries open immutable detail;
- Week slots start creation with date/time prefilled; or
- Month dates start creation with a date prefilled.

Those changes must preserve the active-vehicle selection step, required duration confirmation,
ordinary keyboard/click alternatives, and server authority. They do not permit drag-and-drop,
resizing, recurrence, reminders, resources, or generic events without separate product approval.
