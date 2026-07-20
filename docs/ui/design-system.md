# Shared shell and component contract

The authenticated layout in `assets/views/layouts/base.html` is the only workshop shell. Pages
extend it and provide their content block; they do not duplicate navigation, identity, CSRF, main,
notification, or phone-bar markup. `AuthenticatedLayout` maps collection and record paths to the
owning navigation area. Templates receive only the approved display name, CSRF token, current path,
and derived navigation labels.

The desktop sidebar activates at `64rem`, measured in CSS pixels so browser zoom naturally removes
it when useful content width becomes inadequate. Below that breakpoint the compact header and fixed
phone bar are used. Main content reserves the phone-bar height plus extra space, keeping the final
card, validation, pagination, and form actions reachable.

## Tokens

`assets/static/css/app.css` defines the complete token contract in `:root`: `--color-*`,
`--font-*`, `--space-*`, `--radius-*`, `--border-*`, `--shadow-*`, `--focus-ring`,
`--target-size`, and shell dimensions. Components consume these properties instead of adding
page-specific visual values. Semantic variants use success, warning, and danger tokens and always
include text or another non-color cue.

## Template and CSS classes

| Primitive | Contract |
| --- | --- |
| Buttons | `.button`; add `--secondary`, `--quiet`, or `--destructive`. Use native `disabled` and `aria-busy`. `.primary-button` remains an authentication-compatible alias. |
| Fields | `.field` around a label and input/select/textarea. Use `.field-hint`, `.field-error`, `aria-describedby`, `aria-invalid`, native `readonly`, and `disabled`. Use `.checkbox-field` for a checkbox and label. |
| Filters and cards | `.filter-bar` contains fields/actions. `.card` is the shared bounded surface. |
| Tables | `.data-table` with real table headers; every body cell supplies its header in `data-label` for narrow card rendering. |
| Status | `.badge` with optional semantic modifier; `.panel-state` plus `--empty` or `--error`; `.notification` plus `--success` or `--error`. |
| Record facts | `.definition-list`, with each `dt`/`dd` pair wrapped in a `div`. |
| Pagination | `nav.pagination` with an accessible label, status text, and `.pagination-actions`. Unavailable controls use native `disabled`. |
| Loading | `.htmx-indicator` inside an HTMX trigger or `.loading-indicator` with `role="status"`. Keep existing content present while a bounded region is busy. |
| Dialog/sheet | Native `dialog` with `.dialog-content` and `.dialog-actions`; `data-dialog-open`/`data-dialog-close` only enhance open, close, and focus. Phone More uses native `details` and `.sheet`, so it works without JavaScript. |

The representative fixture is `assets/views/fixtures/components.html`, included on the temporary
authenticated setup page. It covers every primitive and representative disabled, busy, error,
success, and read-only states. When the dashboard replaces that page, retain the fixture for
rendering and regression tests.

Accessibility behavior is part of the contract: native semantics first, 44px minimum controls,
visible `:focus-visible` rings, no horizontal table dependency on phones, 200% text compatibility,
and `prefers-reduced-motion` suppression. JavaScript may coordinate HTMX and enhance dialog/sheet
focus only; submissions and navigation remain standard HTML fallbacks.
