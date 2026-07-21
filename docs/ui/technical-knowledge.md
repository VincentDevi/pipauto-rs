# Technical knowledge

## Knowledge list and search

| Property | Specification |
| --- | --- |
| Route | `GET /knowledge` |
| Access | Authenticated |
| Filters | Full-text query, tags, make, model, engine, Active/Archived |
| Result data | Title, relevant excerpt, tags, make/model/engine context, source indicator, updated date, state |
| Primary action | New technical note → `/knowledge/new` |
| Backend | Combined full-text/structured search with deterministic tie-break and opaque cursor |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Technical knowledge                            [ + New technical note ] │
│ ▌ Knowledge       │ Reuse proven workshop solutions and procedures.                          │
│                   │ [Search titles and notes_____________________] [ Search ]                 │
│                   │ Tags [________] Make [_______] Model [_______] Engine [_______] [Active ▾]│
│                   │                                                                          │
│                   │ Title / match                     Context              Tags       Updated  │
│                   │ Golf Mk7 rear brake service       VW · Golf · 2.0 TDI  brakes     18 Jul  │
│                   │ “Use service mode before…”        Source: 1-ABC-234                         │
│                   │ Transit injector connector fix    Ford · Transit       electrical 12 Jul  │
│                   │ [Previous]                                               [Next]           │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Technical knowledge          │
├──────────────────────────────┤
│ [ + New technical note    ]  │
│ [Search notes_____________]  │
│ [ Filters (2)             ]  │
│ Tags: brakes × · VW ×         │
│                              │
│ ┌──────────────────────────┐ │
│ │ Golf Mk7 rear brake      │ │
│ │ service          ACTIVE  │ │
│ │ VW · Golf · 2.0 TDI      │ │
│ │ brakes · service-mode    │ │
│ │ Use service mode before… │ │
│ └──────────────────────────┘ │
│ [Previous]          [Next]   │
├──────────────────────────────┤
│ Home   Vehicles   Jobs  More │
└──────────────────────────────┘
```

Empty knowledge explains the value of recording a proven solution and offers New technical note.
No matches keeps every filter visible and offers Clear filters. Search relevance stays server-owned;
the interface provides no relevance-sort control unless the API documents one.

## Create and edit technical note

| Property | Specification |
| --- | --- |
| Routes | `GET /knowledge/new`, `GET /knowledge/{id}/edit` |
| Required | Title and body |
| Optional | Normalized tags, make, model, engine, vehicle, source intervention |
| Prefill | From an intervention: its source ID and vehicle; from a vehicle: vehicle and make/model/engine context when supplied |
| Validation | Length/count limits; referenced records exist; source intervention belongs to selected vehicle |
| Actions | Save technical note; Cancel |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Knowledge / New technical note                                            │
│                   │ New technical note                                                         │
│                   │ [error summary or source relationship conflict]                           │
│                   │ Title (required) [_____________________________________________________]   │
│                   │ Body (required)                                                           │
│                   │ [_____________________________________________________________________]   │
│                   │ [_____________________________________________________________________]   │
│                   │ Tags [brakes ×] [service-mode ×] [Add tag____________________________]    │
│                   │ Make [____________] Model [____________] Engine [_____________________]   │
│                   │ Related vehicle [1-ABC-234 · VW Golf ▾]                                  │
│                   │ Source intervention [18 Jul 2026 · Front brakes ▾]                        │
│                   │ [Cancel]                                      [ Save technical note ]   │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ New technical note           │
├──────────────────────────────┤
│ [error summary]              │
│ Title (required)             │
│ [__________________________] │
│ Body (required)              │
│ [__________________________] │
│ [__________________________] │
│ Tags                         │
│ [brakes ×] [service-mode ×]  │
│ [Add tag__________________]  │
│ Vehicle context              │
│ Make [________] Model [____] │
│ Engine [__________________]  │
│ Related vehicle              │
│ [1-ABC-234 · VW Golf ▾]      │
│ Source intervention          │
│ [18 Jul · Front brakes ▾]    │
│ [ Save technical note      ] │
│ Cancel                       │
└──────────────────────────────┘
```

Tags are explicit removable chips plus a text entry; comma parsing is not assumed. Removing the
vehicle while a source intervention remains prompts the user to remove/change the source or restore
the matching vehicle selection. The backend, not browser inference, validates consistency.

## Technical-note detail

| Property | Specification |
| --- | --- |
| Route | `GET /knowledge/{id}` |
| Data | Title/body, tags, structured context, linked vehicle/source intervention, status, dates |
| Primary action | Edit when active |
| Secondary | Archive or Restore; open related records; back to preserved search |
| Restrictions | Archived note is readable and restorable but not ordinarily editable |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Knowledge / Golf Mk7 rear brake service                                  │
│                   │ Golf Mk7 rear brake service [ACTIVE]                [Edit] [Actions ▾]    │
│                   │ brakes · service-mode · VW · Golf · 2.0 TDI                              │
│                   │                                                                          │
│                   │ Use service mode before retracting the electronic parking brake…          │
│                   │ [remaining formatted plain text body]                                    │
│                   │                                                                          │
│                   │ Related vehicle: 1-ABC-234 · Volkswagen Golf →                           │
│                   │ Source: 18 Jul 2026 · Front brake replacement →                          │
│                   │ Updated 18 Jul 2026                                                      │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Technical note               │
├──────────────────────────────┤
│ ACTIVE                       │
│ Golf Mk7 rear brake service  │
│ brakes · service-mode        │
│ VW · Golf · 2.0 TDI          │
│ [Edit] [Actions ▾]           │
│                              │
│ Use service mode before      │
│ retracting the electronic…   │
│                              │
│ Related vehicle              │
│ 1-ABC-234 · VW Golf →        │
│ Source intervention          │
│ 18 Jul · Front brakes →      │
│ Updated 18 Jul 2026          │
├──────────────────────────────┤
│ Home   Vehicles   Jobs  More │
└──────────────────────────────┘
```

The body renders as safe readable text; rich-text editing is not implied. Missing optional context
sections are omitted. Archive confirmation states that the note leaves default search but linked
service history remains intact. Restore returns it to active results.

## Stored attachments

An active note exposes Upload attachment. The shared multipart form requires one file and accepts
optional display name/caption; owner, detected media type, byte size, and storage state are
server-owned. Each stored row exposes Open and Download plus edit/delete actions while active.
Archived notes keep attachment Open/Download access but hide every mutation control until restored.
Attachment changes do not alter tags, search fields, vehicle context, or source-intervention links.

## State and error coverage

- `422`: title, body, tag, and context errors preserve input and tag order.
- `409`: source vehicle/intervention mismatch or concurrent archive/edit; offer Reload latest.
- `404`: note not found, back to Knowledge; missing optional linked record is presented according to
  the backend's safe relationship response rather than crashing the page.
- Expired session: login redirect preserving the local note/search path.
- `503`: search results remain visible during enhanced retry; form errors retain safe input.
- Archived filter is off by default and visually included in the active-filter summary when used.
