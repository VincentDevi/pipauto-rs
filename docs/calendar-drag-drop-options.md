# Calendar and drag-and-drop options

**Status:** Future-milestone research; not an approved product or architecture decision  
**Research checked:** 2026-07-21

## Purpose

Pipauto will eventually need a basic calendar and may later add drag-and-drop. This document
compares practical ways to add those capabilities while preserving the current frontend boundary:
Rust/Tera renders authoritative HTML, ordinary links and forms remain complete without JavaScript,
HTMX provides progressive enhancement, and any additional JavaScript is pinned and self-hosted.

The calendar's product behavior is not defined here. In particular, this document does not decide
what a calendar entry represents or introduce appointments into the initial release.

## Evaluation criteria

An option fits Pipauto better when it:

- leaves the server in control of persisted state and, preferably, rendered HTML;
- works naturally with HTMX fragment swaps and the existing CSRF/error conventions;
- adds little self-hosted JavaScript and no unnecessary frontend framework;
- remains practical on phones and tablets as well as desktop;
- keeps every mutation available through a keyboard-accessible, non-drag form or control;
- can recover safely when a server rejects a move or a request has an uncertain outcome;
- is maintainable by a backend-focused team;
- has clear licensing and can be pinned instead of loaded from a CDN; and
- provides only the calendar capabilities the approved milestone actually needs.

Bundle weight is described qualitatively below. Exact minified and compressed sizes depend on the
version and selected build, so they must be measured from the files proposed for pinning before a
dependency is approved.

## Two decisions, not one

Calendar rendering and drag-and-drop are separate concerns.

A calendar renderer decides how months, weeks, days, time slots, overlapping entries, navigation,
and responsive layouts work. A drag library decides how an existing DOM item can be picked up and
moved between drop targets. [SortableJS] and [Alpine Sort], for example, reorder items in one or
more collections. They do not provide calendar date calculations, a week/day time grid, event
resizing, overlap rules, recurrence, or timezone behavior.

This distinction makes a small, server-rendered month calendar compatible with SortableJS when a
move means "put this card in another day." It does not make SortableJS a substitute for a scheduling
calendar when a pointer position must become an exact start time or duration.

## Options

### 1. Server-rendered HTMX calendar with native HTML Drag and Drop

Rust computes the visible range and Tera renders the calendar. Normal GET links or forms navigate
between ranges. Calendar entries use the browser's [HTML Drag and Drop API], while drop handlers
submit the entry identifier and destination to the server.

**Advantages**

- No third-party JavaScript dependency.
- The server continues to own both HTML and persisted state.
- Natural fit for an HTMX-rendered month grid with date cells as drop targets.
- Smallest likely download when the interaction is extremely limited.

**Disadvantages**

- The native API is mouse-oriented and has awkward or inconsistent touch behavior, making it a
  risky foundation for Pipauto's phone/tablet requirement.
- Pipauto would own drag feedback, cancellation, drop validation, DOM restoration, HTMX lifecycle
  integration, and browser quirks.
- It does not provide keyboard interaction or the single-pointer alternative required by
  [WCAG 2.2 dragging guidance]; an ordinary move form is still required.
- It only handles drop targets. Time-slot calculations, resizing, overlaps, and other calendar
  behavior remain custom work.

**Fit:** Possible for a desktop-only experiment, but not recommended as Pipauto's cross-device
implementation.

**License and weight:** Browser platform API; no library license or dependency weight.

### 2. Server-rendered HTMX calendar with custom Pointer Events

Rust/Tera and HTMX still own the calendar. A custom module uses [Pointer Events] to implement a
unified mouse, pen, and touch gesture rather than the native HTML drag model.

**Advantages**

- Full control over touch thresholds, long-press behavior, handles, previews, and valid targets.
- Can be designed specifically for Pipauto's calendar markup and server endpoints.
- Avoids adopting a general frontend framework or calendar package.
- The server can remain authoritative and return the refreshed calendar fragment after a move.

**Disadvantages**

- Highest custom interaction cost: gesture recognition, scrolling versus dragging, capture,
  cancellation, auto-scroll, visual feedback, reduced motion, and cleanup all become Pipauto code.
- Accessibility still needs separate keyboard and single-click/tap controls.
- Calendar-specific placement and resizing would substantially increase the amount of custom code.
- A small first version can grow into a bespoke drag library that the team must test and maintain.

**Fit:** A fallback only if a later, tightly scoped requirement cannot be met cleanly by SortableJS.

**License and weight:** Browser platform API; no third-party license, but a medium-to-high amount of
application JavaScript and test code.

### 3. Server-rendered HTMX calendar with SortableJS

Rust/Tera renders a month or list-style calendar. Each day is a connected SortableJS collection,
allowing entry cards to move between days or change their order within a day. A drop sends the
requested move to a CSRF-protected server endpoint and replaces or reloads the authoritative
calendar region.

**Advantages**

- Purpose-built, framework-free library for reorderable lists and shared lists.
- Supports touch devices, handles, filtering, animation, and connected groups without adding a UI
  framework.
- Small-to-medium dependency and substantially less custom gesture code than Pointer Events.
- Works with server-rendered markup; only the calendar region needs initialization.
- Good match when the complete requirement is moving date-only cards between day buckets.

**Disadvantages**

- It is a list/grid sorting library, not a scheduling calendar.
- It cannot independently translate vertical pointer position into a time, resize duration, lay out
  overlaps, or handle recurrence.
- It mutates the DOM optimistically, so rejected moves, conflicts, and transport uncertainty need
  an explicit revert or authoritative fragment refresh.
- Instances must be initialized after the initial load and every relevant HTMX swap, and destroyed
  when necessary to avoid duplicate handlers.
- Its drag UI does not remove the need for keyboard and click/tap alternatives.

**Fit:** Best drag layer if the approved requirement is limited to moving entries between dates or
ordering entries inside date buckets.

**License and weight:** MIT; small-to-medium standalone dependency. Use a pinned, self-hosted build
from the [SortableJS repository], not a CDN.

### 4. Server-rendered HTMX calendar with Alpine.js and Alpine Sort

The calendar remains server-rendered, but Alpine core and its Sort plugin provide declarative
`x-sort` attributes and callbacks. [Alpine Sort explicitly uses SortableJS] for its drag behavior.

**Advantages**

- Concise markup for sortable items, groups, handles, ignored controls, and drop callbacks.
- Offers Alpine's broader declarative state model if Pipauto later approves several other Alpine
  use cases.
- Retains the same underlying SortableJS capabilities for touch and connected lists.

**Disadvantages**

- Adds Alpine core and a wrapper plugin on top of the SortableJS behavior Pipauto actually needs.
- Introduces a second frontend programming model alongside HTMX and the existing small JavaScript
  module.
- Has the same calendar limitations as SortableJS: no time grid, resizing, overlap layout, or
  calendar semantics.
- Alpine and HTMX lifecycle interaction becomes another convention the team must learn and test.
- The additional abstraction provides little value when sorting is the only Alpine feature.

**Fit:** Do not introduce Alpine solely for calendar drag-and-drop. Reconsider only if Alpine is
independently approved as Pipauto's general-purpose client-side behavior layer.

**License and weight:** Alpine and SortableJS are MIT-licensed. Combined weight is higher than using
SortableJS directly because Alpine core and the Sort plugin are both required.

### 5. FullCalendar Standard as an isolated JavaScript calendar

A server-rendered page reserves a calendar island. FullCalendar owns that island, loads entry data
from a JSON event feed, and reports moves through callbacks such as [`eventDrop`]. The server still
validates and persists every mutation; the callback's `revert()` support can undo rejected moves.

**Advantages**

- Mature calendar engine with month, list, week, and day/time-grid views.
- Built-in event dragging, resizing, touch support, constraints, and overlap controls.
- Avoids building calendar geometry and scheduling interactions from scratch.
- Standard features are MIT-licensed, and prebuilt browser bundles are available without requiring
  Pipauto to adopt React, Vue, or Angular.

**Disadvantages**

- FullCalendar, rather than Tera, owns the island's rendered HTML and interaction state.
- Requires a JSON presentation boundary in addition to Pipauto's normal HTML fragments.
- Significantly more JavaScript and CSS than a server-rendered month grid with SortableJS.
- Styling, accessibility verification, error recovery, focus behavior, and synchronization after
  surrounding HTMX swaps require an explicit integration layer.
- Resource timeline and resource time-grid views are Premium features with separate licensing;
  they must not be assumed to be available under the Standard license.

**Fit:** Best escalation path if approved requirements include timed week/day scheduling, exact
placement, resizing, collision rules, or other interactions that would otherwise create a custom
calendar engine. Keep it isolated rather than converting the whole frontend to client rendering.

**License and weight:** FullCalendar Standard is MIT according to its [license page]; Premium
features have different terms. This is a large dependency relative to Pipauto's current frontend.

### 6. DayPilot Lite

[DayPilot Lite] is an open-source suite of plain-JavaScript calendar and scheduler components. Its
Lite calendar supports day/week time-axis views and drag creation, moving, and resizing; its Lite
scheduler adds resource timelines and post-drop validation.

**Advantages**

- Provides timed calendar and scheduling behavior, touch support, resizing, and server validation
  hooks without requiring a frontend framework.
- The Lite scheduler includes resource-oriented views that may be relevant if a future approved
  design schedules mechanics, bays, tools, or vehicles.
- Plain JavaScript integration is possible.

**Disadvantages**

- Client-side component owns calendar rendering and state, with the same architectural tension as
  FullCalendar.
- The Lite/Pro feature boundary must be checked carefully against the exact future requirements.
- Adds a substantial specialized UI dependency and a separate styling model.
- Smaller ecosystem and less direct HTMX precedent than FullCalendar.
- Resource concepts are not part of Pipauto's current product requirements and must not be inferred
  from the library's capabilities.

**Fit:** Credible secondary evaluation for a genuinely resource-oriented or time-grid milestone,
but not justified for the currently planned basic calendar.

**License and weight:** DayPilot Lite is Apache License 2.0. It is a large dependency relative to a
custom month grid.

### 7. TOAST UI Calendar

[TOAST UI Calendar] is a client-rendered calendar with month, week, day, multi-week, drag, and
resize support. Its documented runtime dependencies include Preact, Immer, and DOMPurify.

**Advantages**

- Full calendar behavior, including timed views, dragging, resizing, themes, and default popups.
- Plain-JavaScript package is available in addition to framework wrappers.
- MIT license.

**Disadvantages**

- Heavier dependency graph and client-side rendering model conflict with Pipauto's preference for
  server-owned HTML and minimal JavaScript.
- Its built-in state and popups would either duplicate or need adaptation to Pipauto's Tera/HTMX
  forms, validation, CSRF, and focus conventions.
- The project documents optional usage-statistics collection, which would need to be explicitly
  disabled and verified for Pipauto.
- Offers much more frontend machinery than a basic calendar needs.

**Fit:** Technically capable but a weaker architectural fit than FullCalendar or DayPilot Lite. Keep
only as a benchmark unless future evaluation reveals a unique required capability.

**License and weight:** MIT; large relative dependency because the calendar and its runtime
dependencies are client-side.

### 8. Schedule-X

[Schedule-X] is a modern client-side calendar with framework integrations and an MIT-licensed core.
Its current [drag-and-drop plugin] is a Premium package that requires an active license.

**Advantages**

- Modern responsive calendar, multiple views, extensibility, and active development.
- Can be used from plain JavaScript as well as popular frontend frameworks.
- Plugin architecture allows features to be selected.

**Disadvantages**

- Drag-and-drop—the feature being evaluated—is currently Premium, adding commercial licensing and
  package-authentication concerns.
- Client-rendered architecture and plugin state are less aligned with Tera/HTMX than a server grid.
- Introducing it for this milestone would add both a calendar framework and a paid interaction
  dependency.
- Several advanced scheduling capabilities are also Premium and must not influence product scope.

**Fit:** Not shortlisted while drag-and-drop requires a Premium plugin and simpler open-source
choices satisfy the likely requirements.

**License and weight:** MIT core; commercial terms apply to the Premium drag-and-drop plugin. Large
relative to the current frontend once the calendar and required plugins are included.

## Decision matrix

| Option | HTML owner | Views supplied | Timed placement / resize | Touch | Accessibility effort | HTMX fit | JS weight | Maintenance |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Native HTML drag | Server | None; Pipauto builds them | No / no | Weak-risky | Very high | High | Very small | High custom burden |
| Custom Pointer Events | Server | None; Pipauto builds them | Custom / custom | Can be strong | Very high | High | Small-medium custom | Highest custom burden |
| SortableJS | Server | None; Pipauto builds them | Day buckets only / no | Built in | Medium-high | High | Small-medium | Low-medium |
| Alpine + Sort | Server | None; Pipauto builds them | Day buckets only / no | Built in | Medium-high | Medium-high | Medium | Medium; two abstractions |
| FullCalendar Standard | Client island | Month, list, week, day | Yes / yes | Built in | Medium; verify widget | Medium | Large | Library upgrades + adapter |
| DayPilot Lite | Client island | Month and timed/scheduler views | Yes / yes | Built in | Medium; verify widget | Medium | Large | Library/edition boundary |
| TOAST UI Calendar | Client island | Month, week, day, multi-week | Yes / yes | Library-managed | Medium; verify widget | Low-medium | Large | Dependency/integration burden |
| Schedule-X | Client island | Calendar views | Premium / premium resize | Plugin-managed | Medium; verify widget | Low-medium | Large | License + plugin upgrades |

"Accessibility effort" includes keyboard operation, a non-drag single-pointer alternative,
announcements, focus recovery, reduced motion, and verification with assistive technology. No
library removes Pipauto's responsibility for those outcomes.

## Assessment of the supplied HTMX references

### Jonathan Lahijani's HTMX Event Calendar

The [HTMX Event Calendar article] is the strongest reference for Pipauto's calendar foundation. It
demonstrates server-generated monthly HTML, CSS Grid layout, and HTMX replacement of the calendar
region during navigation. That matches Pipauto's preferred ownership model and avoids a client-side
calendar engine.

Pipauto should adopt the pattern, not copy the implementation literally:

- calendar navigation should use ordinary GET links or a GET form because it is read-only;
- every link should work as a full-page request and optionally target the calendar fragment with
  HTMX;
- Rust should calculate validated date ranges and Tera should render the established page/fragment
  pair;
- the URL should reproduce the visible range and work with refresh, back, and copied links; and
- phone behavior, focus after swaps, empty states, and entry actions should follow Pipauto's own
  frontend guide.

The article is about calendar rendering and navigation. It does not solve drag-and-drop or
server-authoritative mutation recovery.

### `rajasegar/htmx-calendar`

The [`rajasegar/htmx-calendar` repository] is useful only as prototype inspiration. Inspection of
the referenced repository on the research date found:

- no drag-and-drop implementation;
- no persisted event store and an explicit README note that events are not persisted;
- no test suite and no published releases;
- an Express/Pug server with process-global current month/year state;
- Bootstrap, HTMX, and Hyperscript loaded from CDNs; and
- modal/event behavior coupled to the prototype's markup and client scripts.

Those choices are unsuitable for Pipauto's concurrency, self-hosting, server-authority, security,
testing, and progressive-enhancement conventions. The repository must not be added as a dependency
or used as production calendar code.

### Alpine Sort versus SortableJS

Alpine's documentation states that the Sort plugin's drag behavior is provided by SortableJS.
Alpine adds declarative directives and handlers but does not add calendar semantics. If Pipauto only
needs connected sortable day buckets, direct SortableJS integration is the smaller and clearer
choice. Alpine should be reconsidered only through a separate decision supported by multiple
approved use cases.

## Recommendation

### Ranked path

1. **Build the calendar foundation with Rust/Tera and HTMX.** Render authoritative month HTML on the
   server, use normal GET navigation, and make complete edit/move forms the baseline interaction.
2. **Add pinned, self-hosted SortableJS only if the approved drag requirement is date-bucket
   movement or ordering.** Initialize it on the calendar root after initial load and HTMX swaps.
3. **Escalate to an isolated FullCalendar Standard island if requirements demand time-grid
   scheduling.** Prefer the mature engine over implementing time geometry, resizing, collision
   handling, and complex gesture behavior in application JavaScript.

Do not add Alpine solely for sorting, adopt `rajasegar/htmx-calendar`, or build custom pointer
gestures unless later evidence shows that SortableJS cannot meet a tightly bounded requirement.
DayPilot Lite remains a secondary time-grid/resource benchmark; TOAST UI Calendar and Schedule-X
are not preferred under the current constraints.

### Mutation and recovery contract

Whatever drag layer is chosen, dragging is only a shortcut for a server-authoritative mutation:

- Keep an ordinary edit/move form that works without JavaScript and satisfies keyboard and
  single-pointer accessibility requirements.
- Submit only server-issued opaque identifiers and validated destination values.
- Send unsafe requests to same-origin, CSRF-protected endpoints using Pipauto's existing conventions.
- Validate lifecycle, date/time, conflict, and authorization rules on the server; never trust the
  DOM position.
- On success, replace or reload the smallest authoritative calendar region rather than treating the
  optimistic DOM order as persisted truth.
- On a validation or conflict response, restore the server-rendered state and announce why the move
  was rejected.
- On a timeout or transport failure, treat the result as uncertain, reload the authoritative state,
  and tell the user to verify it before retrying.

## Decisions required before implementation

The future milestone must answer these questions before choosing a calendar package or designing
its mutation endpoint:

- What domain record appears on the calendar?
- Are entries date-only, timed, or both?
- Is the first calendar month-only, or are week/day views required?
- Does dragging change only the date, or also start time, duration, and ordering?
- Is resizing required?
- Can entries overlap, and if not, how are conflicts resolved?
- Is recurrence required, and what does moving one occurrence mean?
- Which timezone defines stored instants, displayed days, daylight-saving transitions, and
  all-day entries?
- Are resources such as mechanics or workshop bays actually required?

These are product and data-model decisions, not defaults to infer from library feature lists.

## Sources

- [Alpine Sort](https://alpinejs.dev/plugins/sort)
- [SortableJS documentation and source](https://github.com/SortableJS/Sortable)
- [Jonathan Lahijani: HTMX Event Calendar](https://jonathanlahijani.com/posts/htmx-event-calendar/)
- [`rajasegar/htmx-calendar`](https://github.com/rajasegar/htmx-calendar)
- [HTML Drag and Drop API](https://developer.mozilla.org/en-US/docs/Web/API/HTML_Drag_and_Drop_API)
- [Pointer Events](https://developer.mozilla.org/en-US/docs/Web/API/Pointer_events)
- [WCAG 2.2: Understanding dragging movements](https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements)
- [FullCalendar event dragging and resizing](https://fullcalendar.io/docs/event-dragging-resizing)
- [FullCalendar touch support](https://fullcalendar.io/docs/touch)
- [FullCalendar licensing](https://fullcalendar.io/license)
- [DayPilot Lite](https://javascript.daypilot.org/open-source/)
- [TOAST UI Calendar](https://ui.toast.com/tui-calendar/)
- [Schedule-X](https://schedule-x.dev/)
- [Schedule-X drag-and-drop plugin](https://schedule-x.dev/docs/calendar/plugins/drag-and-drop)

[Alpine Sort]: https://alpinejs.dev/plugins/sort
[Alpine Sort explicitly uses SortableJS]: https://alpinejs.dev/plugins/sort
[SortableJS]: https://sortablejs.github.io/Sortable/
[SortableJS repository]: https://github.com/SortableJS/Sortable
[HTML Drag and Drop API]: https://developer.mozilla.org/en-US/docs/Web/API/HTML_Drag_and_Drop_API
[Pointer Events]: https://developer.mozilla.org/en-US/docs/Web/API/Pointer_events
[WCAG 2.2 dragging guidance]: https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements
[`eventDrop`]: https://fullcalendar.io/docs/eventDrop
[license page]: https://fullcalendar.io/license
[DayPilot Lite]: https://javascript.daypilot.org/open-source/
[TOAST UI Calendar]: https://ui.toast.com/tui-calendar/
[Schedule-X]: https://schedule-x.dev/
[drag-and-drop plugin]: https://schedule-x.dev/docs/calendar/plugins/drag-and-drop
[HTMX Event Calendar article]: https://jonathanlahijani.com/posts/htmx-event-calendar/
[`rajasegar/htmx-calendar` repository]: https://github.com/rajasegar/htmx-calendar
