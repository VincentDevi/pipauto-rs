# Interventions and service history

## Interventions list

| Property | Specification |
| --- | --- |
| Route | `GET /interventions` |
| Access | Authenticated |
| Entry/exit | Sidebar/bottom Jobs, dashboard queues, vehicle history; opens detail or create flow |
| Filters | Text where supported, vehicle, customer where supported, status, service-date range |
| Result data | Service date, vehicle/registration, customer when supplied, mileage, short work/problem summary, status, financial total |
| Primary action | New intervention; select an active vehicle, then use `/vehicles/{id}/interventions/new` |
| Backend | Stable cursor list ordered by documented service-date tuple; cancelled entries visible when requested/default contract supplies them |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Interventions                                    [ + New intervention ] │
│ ▌ Interventions   │ [Search work or vehicle________________________] [ Search ]              │
│                   │ [Status: All ▾] [Vehicle ▾] [From ____] [To ____] [Clear]                │
│                   │                                                                          │
│                   │ Date        Vehicle       Mileage     Summary              Status   Total │
│                   │ 18 Jul 2026  1-ABC-234    126,400 km  Front brakes         COMPLETED €240 │
│                   │ 17 Jul 2026  2-DEF-567     88,200 km  Annual service       DRAFT     €80  │
│                   │ 10 Feb 2026  1-ABC-234    118,000 km  Engine inspection    CANCELLED —    │
│                   │ [Previous]                                               [Next]           │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Interventions                │
├──────────────────────────────┤
│ [ + New intervention       ] │
│ [Search work or vehicle___]  │
│ [ Filters (Status: All)    ] │
│                              │
│ ┌──────────────────────────┐ │
│ │ COMPLETED · 18 Jul 2026  │ │
│ │ 1-ABC-234 · VW Golf      │ │
│ │ 126,400 km               │ │
│ │ Front brakes       €240  │ │
│ └──────────────────────────┘ │
│ ┌──────────────────────────┐ │
│ │ DRAFT · 17 Jul 2026      │ │
│ │ 2-DEF-567 · Fiat Panda   │ │
│ │ Annual service       €80 │ │
│ └──────────────────────────┘ │
│ [Previous]          [Next]   │
├──────────────────────────────┤
│ Home   Vehicles   Jobs  More │
└──────────────────────────────┘
```

No matches keeps filters and offers Clear filters. A completely empty set offers vehicle selection
for a first intervention. Cursor pagination preserves deterministic server ordering.

## Create and edit draft intervention

| Property | Specification |
| --- | --- |
| Routes | `GET /vehicles/{id}/interventions/new`, `GET /interventions/{id}/edit` |
| Required | Active vehicle, service date, backend-required workshop content before completion |
| Optional | Mileage, customer-reported problem, diagnostics, performed work, recommendations, general notes |
| Actions | Save draft; Cancel navigation; lines are managed after initial draft creation |
| Validation | Non-negative mileage; chronology against neighboring non-cancelled records; bounded text; immutable state conflicts |
| Backend | Create/update draft and chronology validation; authoritative current mileage update |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Vehicles / 1-ABC-234 / New intervention                                  │
│                   │ New intervention for Volkswagen Golf                                      │
│                   │ Owner: Mario Rossi · Current vehicle mileage: 126,400 km                  │
│                   │ [error summary or chronology conflict]                                   │
│                   │                                                                          │
│                   │ Service date (required) [2026-07-19]  Recorded mileage [________] km      │
│                   │ Customer-reported problem [____________________________________________]  │
│                   │ Diagnostics              [____________________________________________]  │
│                   │ Work performed           [____________________________________________]  │
│                   │ Recommendations          [____________________________________________]  │
│                   │ General notes            [____________________________________________]  │
│                   │                                                                          │
│                   │ [Cancel]                                              [ Save draft ]     │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ New intervention             │
├──────────────────────────────┤
│ Volkswagen Golf              │
│ 1-ABC-234 · Mario Rossi      │
│ Current: 126,400 km          │
│ [error summary]              │
│ Service date (required)      │
│ [2026-07-19______________]   │
│ Recorded mileage [______] km │
│ Reported problem             │
│ [__________________________] │
│ Diagnostics                  │
│ [__________________________] │
│ Work performed               │
│ [__________________________] │
│ Recommendations              │
│ [__________________________] │
│ General notes                │
│ [__________________________] │
│ [ Save draft               ] │
│ Cancel                       │
└──────────────────────────────┘
```

Creation returns to draft detail. Edit uses **Save changes**. A backdated-mileage conflict preserves
all fields, links to the vehicle history, and identifies the date/mileage rule without changing
another record. An archived-vehicle conflict sends the user back to read-only vehicle detail.

## Draft intervention detail

| Property | Specification |
| --- | --- |
| Route | `GET /interventions/{id}` |
| Data | Vehicle/owner, date, mileage, narrative fields, ordered lines, totals, metadata attachments, state timestamps |
| Primary action | Complete intervention |
| Secondary actions | Edit details, Add line item, Add attachment metadata, Create technical note, Create invoice draft, Cancel intervention |
| Restrictions | All ordinary fields and line mutations require Draft state |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Interventions / 18 Jul 2026 · 1-ABC-234                                  │
│                   │ Front brake replacement [DRAFT]       [Complete intervention] [Actions ▾] │
│                   │ Volkswagen Golf → · Mario Rossi · 126,400 km                             │
│                   │                                                                          │
│                   │ Reported problem              Diagnostics                                │
│                   │ Grinding under braking.       Front pads and discs worn.                 │
│                   │ Work performed / Recommendations / Notes                 [Edit details]  │
│                   │                                                                          │
│                   │ Line items                                             [ + Add line ]    │
│                   │ Type      Description             Qty   Unit   Price    Cost    Total      │
│                   │ PART      Front brake discs       2     each   €70      €45     €140       │
│                   │ LABOUR    Brake replacement       2     hour   €50      —       €100       │
│                   │                                                        Total    €240       │
│                   │                                                                          │
│                   │ Attachment metadata [Add metadata]  Technical knowledge [Create note]    │
│                   │ inspection.jpg · image/jpeg · METADATA ONLY                              │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Intervention                 │
├──────────────────────────────┤
│ DRAFT · 18 Jul 2026          │
│ Front brake replacement      │
│ 1-ABC-234 · VW Golf →        │
│ 126,400 km                   │
│ [ Complete intervention    ] │
│ [Edit details] [Actions ▾]   │
│                              │
│ Reported problem             │
│ Grinding under braking.      │
│ Diagnostics                  │
│ Front pads and discs worn.   │
│                              │
│ Line items          [ + Add] │
│ ┌──────────────────────────┐ │
│ │ PART · Front discs       │ │
│ │ 2 each × €70      €140   │ │
│ │ [Edit] [Remove]          │ │
│ └──────────────────────────┘ │
│ Total                  €240  │
│                              │
│ Attachments · METADATA ONLY  │
│ inspection.jpg               │
│ [Add metadata]               │
├──────────────────────────────┤
│ Home   Vehicles   Jobs  More │
└──────────────────────────────┘
```

## Intervention line item form

The modal on desktop and full-screen sheet on phone contains category (labour, part, material, or
other), description, positive quantity (up to three fractional digits), unit label, non-negative
unit price, optional non-negative unit cost, and position. Currency is displayed from the
intervention and is not independently selectable.

```text
Desktop dialog                                 Phone sheet
┌───────────────────────────────────────────┐  ┌──────────────────────────────┐
│ Add line item                         [×] │  │ Add line item           [×] │
│ Category [Part ▾]                         │  │ Category [Part ▾]            │
│ Description [__________________________]  │  │ Description                  │
│ Quantity [____] Unit [each___________]    │  │ [__________________________] │
│ Unit price [________] EUR                 │  │ Quantity [____]              │
│ Unit cost  [________] EUR (optional)      │  │ Unit [____________________]  │
│ Position [__]                             │  │ Unit price [_______] EUR     │
│ [Cancel]                  [ Add line ]    │  │ Unit cost  [_______] EUR     │
└───────────────────────────────────────────┘  │ [ Add line                ]  │
                                               └──────────────────────────────┘
```

The server calculates and returns totals. Reordering updates stable positions through an explicit
control: Move up/Move down, never drag-only interaction. Remove requires a confirmation naming the
line and recalculates totals atomically.

## Complete and cancel transitions

Complete confirmation shows vehicle, service date, recorded mileage, total, and a checklist-style
summary of work content. Its primary text is **Complete and lock intervention**. It explains that
ordinary fields and lines become read-only and completion cannot be undone in this release.

Cancel confirmation requires no invented reason field. It explains that the intervention remains
visible as Cancelled and cannot return to Draft. Its destructive action is **Cancel intervention**.

Concurrent transition or stale-draft `409` closes the busy state and reloads the authoritative
status. Only a still-valid Draft continues to show mutation controls.

## Completed and cancelled detail

Both reuse the detail page with all form and line mutation controls removed. Completed shows its
completion timestamp and permits Create technical note and Create invoice draft when backend
relationship rules allow it. Cancelled shows cancellation timestamp and does not present invoice
creation as a primary next step.

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Interventions / 18 Jul 2026 · 1-ABC-234                                  │
│                   │ Front brake replacement [COMPLETED]                   [Actions ▾]         │
│                   │ Completed 18 Jul 2026 · Volkswagen Golf → · 126,400 km                   │
│                   │ [Read-only narrative sections]                                            │
│                   │ [Read-only ordered line table]                         Total €240          │
│                   │ [Create technical note] [Create invoice draft]                            │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Intervention                 │
├──────────────────────────────┤
│ COMPLETED · 18 Jul 2026      │
│ Front brake replacement      │
│ 1-ABC-234 · 126,400 km       │
│ Completed 18 Jul 2026        │
│                              │
│ [Read-only work sections]    │
│ [Read-only line cards]       │
│ Total                  €240  │
│ [Create technical note]      │
│ [Create invoice draft]       │
├──────────────────────────────┤
│ Home   Vehicles   Jobs  More │
└──────────────────────────────┘
```

## Full vehicle service history

| Property | Specification |
| --- | --- |
| Route | `GET /vehicles/{id}/history` |
| Order | `service_date DESC`, `created_at DESC`, `id DESC`; never resort client-side |
| Filters | Status and date range only when backend route documents them |
| Rows/cards | Date, status, recorded mileage, concise work/problem, financial summary, detail link |
| State | Cancelled remains visually distinct; identical dates retain server order across cursors |

The desktop layout is the full-width history table from vehicle detail with filters and cursor
controls. The phone layout is the history card stream from vehicle detail with a sticky record
identity header, not a separate navigation shell. Back to vehicle preserves the history cursor only
when returning to history; the vehicle detail itself always opens at its top.

## Attachment metadata

Intervention attachment metadata uses the same form and **Metadata only** warning as vehicle
attachments. The owner is derived from this intervention and is not editable. No binary picker,
camera action, preview, thumbnail, download, checksum, or uploaded-state badge is present.

## State and error coverage

- `422`: inline narrative, line, date, mileage, quantity, and money errors with preserved input.
- `409`: chronology, immutable-state, concurrent transition, or line-total conflict with Reload.
- `404`: intervention not found, back to Interventions; missing vehicle during create, back to
  Vehicles.
- Expired session: safe login redirect with local return path.
- `503`: retry panel; draft input retained only when it can be kept without exposing sensitive data.
- Empty lines/attachments: explain optional next action; completion validation remains authoritative.


