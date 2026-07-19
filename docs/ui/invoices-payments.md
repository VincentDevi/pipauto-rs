# Invoices and payments

## Product boundary

This design is tax-neutral. It does not label output as legally compliant, calculate VAT/tax, send
email, integrate a payment provider, support refunds/corrections, or promise export. Currency and
all authoritative amounts come from the backend. Export appears only as explanatory unavailable
text until a separately approved capability exists.

## Invoices list

| Property | Specification |
| --- | --- |
| Route | `GET /invoices` |
| Access | Authenticated |
| Filters | Text/number where supported, lifecycle status, derived payment status, customer, issue-date range |
| Result data | Draft ID or final number, customer snapshot/name, issue/due date, lifecycle, total, paid, outstanding, payment status |
| Primary action | New invoice → `/invoices/new` |
| Backend | Filtered invoice listing and opaque cursor; derived payment values supplied by backend |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Invoices                                             [ + New invoice ] │
│ ▌ Invoices        │ [Search invoice or customer_____________________] [ Search ]              │
│                   │ [Lifecycle: All ▾] [Payment: Outstanding ▾] [From ____] [To ____]         │
│                   │                                                                          │
│                   │ Number/draft  Customer       Issued     Due       Status   Total  Balance │
│                   │ 2026-00012    Mario Rossi    18 Jul     01 Aug    UNPAID   €240   €240    │
│                   │ 2026-00011    G. Bianchi     12 Jul     26 Jul    PARTIAL  €180   €80     │
│                   │ Draft · a1b…  Mario Rossi    —          —         DRAFT    €75    —       │
│                   │ [Previous]                                               [Next]           │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Invoices                     │
├──────────────────────────────┤
│ [ + New invoice           ]  │
│ [Search invoices__________]  │
│ [ Filters (Outstanding)   ]  │
│                              │
│ ┌──────────────────────────┐ │
│ │ 2026-00012       UNPAID  │ │
│ │ Mario Rossi              │ │
│ │ Issued 18 Jul · Due 1 Aug│ │
│ │ Total €240 · Due €240    │ │
│ └──────────────────────────┘ │
│ ┌──────────────────────────┐ │
│ │ 2026-00011       PARTIAL │ │
│ │ G. Bianchi               │ │
│ │ Total €180 · Due €80     │ │
│ └──────────────────────────┘ │
│ [Previous]          [Next]   │
├──────────────────────────────┤
│ Home   Vehicles   Jobs  More │
└──────────────────────────────┘
```

Empty invoices offers New invoice. No matches keeps filters and offers Clear filters. Drafts never
display or predict a final number. Outstanding is the derived Unpaid/Partially paid state, not an
independently editable flag.

## Create invoice draft

| Property | Specification |
| --- | --- |
| Route | `GET /invoices/new` |
| Required | Active customer and currency supplied/defaulted by backend |
| Optional | Related vehicle, related intervention, due date, notes; relationship combination must be valid |
| Prefill | From intervention: customer, vehicle, intervention; from customer/vehicle: available relationship context |
| Actions | Create draft; Cancel |
| Backend | Validate active customer and current customer→vehicle→intervention relationships at draft time |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Invoices / New invoice                                                    │
│                   │ New invoice draft                                                          │
│                   │ [error summary or relationship conflict]                                 │
│                   │ Customer (required) [Search active customers__________________________]   │
│                   │ Vehicle (optional) [Vehicles belonging to customer ▾]                     │
│                   │ Intervention (optional) [Interventions for selected vehicle ▾]            │
│                   │ Currency [EUR] (set by workshop configuration)                            │
│                   │ Due date (optional) [__________]                                          │
│                   │ Notes [________________________________________________________________]  │
│                   │ [Cancel]                                           [ Create draft ]      │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ New invoice draft            │
├──────────────────────────────┤
│ [error summary]              │
│ Customer (required)          │
│ [Search active customers__]  │
│ Vehicle (optional)           │
│ [Customer vehicles ▾]        │
│ Intervention (optional)      │
│ [Vehicle interventions ▾]    │
│ Currency                     │
│ EUR · workshop setting       │
│ Due date (optional)          │
│ [__________________________] │
│ Notes                        │
│ [__________________________] │
│ [ Create draft             ] │
│ Cancel                       │
└──────────────────────────────┘
```

Changing customer clears incompatible vehicle/intervention choices only after an explicit warning
if the user already selected them. A `409` preserves selections, describes the changed relationship,
and requires choosing a currently valid combination.

## Draft invoice detail and editing

| Property | Specification |
| --- | --- |
| Routes | `GET /invoices/:id`, `GET /invoices/:id/edit` for Draft |
| Data | Customer/current related-record references, currency, due date, notes, ordered lines, subtotal/total |
| Primary action | Issue invoice once valid and non-empty |
| Secondary | Edit header, Add/edit/remove/reorder line, Void only if backend permits Draft voiding, return to source intervention |
| Restrictions | Draft has no issue number/date and cannot receive payments |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Invoices / Draft · a1b…                                                   │
│                   │ Invoice draft [DRAFT]                           [Issue invoice] [Actions ▾]│
│                   │ Customer: Mario Rossi → · Vehicle: 1-ABC-234 → · Intervention: 18 Jul → │
│                   │ Due date: 1 Aug 2026 · Currency: EUR                    [Edit header]     │
│                   │                                                                          │
│                   │ Invoice lines                                         [ + Add line ]    │
│                   │ Description                  Qty   Unit   Price              Total         │
│                   │ Front brake discs            2     each   €70                €140          │
│                   │ Brake replacement labour     2     hour   €50                €100          │
│                   │                                                      Subtotal €240        │
│                   │                                                         Total €240        │
│                   │                                                                          │
│                   │ Export becomes available only when backend support is implemented.        │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Invoice draft                │
├──────────────────────────────┤
│ DRAFT · a1b…                 │
│ Mario Rossi →                │
│ 1-ABC-234 · 18 Jul job →     │
│ Due 1 Aug 2026 · EUR         │
│ [ Issue invoice           ]  │
│ [Edit header] [Actions ▾]    │
│                              │
│ Invoice lines       [ + Add] │
│ ┌──────────────────────────┐ │
│ │ Front brake discs        │ │
│ │ 2 each × €70      €140   │ │
│ │ [Edit] [Remove]          │ │
│ └──────────────────────────┘ │
│ Subtotal               €240  │
│ Total                  €240  │
│                              │
│ Export unavailable           │
│ Backend support is required. │
├──────────────────────────────┤
│ Home   Vehicles   Jobs  More │
└──────────────────────────────┘
```

The invoice-line dialog contains description, positive quantity, unit label, non-negative unit
price, position, and an optional read-only source-intervention-line reference. Currency matches the
invoice. The browser may show returned calculated line total but never owns rounding. Move up/down
is keyboard accessible. Header editing uses the create form's optional fields; changing immutable
identity relationships after backend-permitted draft creation follows the documented API contract.

## Issue confirmation

The dialog repeats customer, related vehicle/intervention, line count, total, and due date. It says:

> Issuing assigns a final number and freezes the customer snapshot, billing snapshot, and lines.
> This cannot be returned to Draft in this release.

The primary action is **Issue and lock invoice**. Empty drafts cannot open the confirmation. A
concurrent `409` reloads authoritative lifecycle/total and never repeats issue-number allocation.

## Issued invoice detail

| Property | Specification |
| --- | --- |
| Route | `GET /invoices/:id` |
| Data | Final number, issued/customer/billing snapshots, lines, subtotal/total, paid, outstanding, derived status, immutable relations, payments |
| Primary action | Record payment while outstanding and non-void |
| Secondary | Void when allowed; open source records; export unavailable explanation |
| Restrictions | Header/snapshots/lines/number are read-only; paid invoice cannot be voided in this release |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Invoices / 2026-00012                                                     │
│                   │ Invoice 2026-00012 [ISSUED] [PARTIALLY PAID]    [Record payment] [Actions]│
│                   │ Issued 18 Jul 2026 · Due 1 Aug 2026 · EUR                               │
│                   │                                                                          │
│                   │ Bill to snapshot                  Related records                         │
│                   │ Mario Rossi                       1-ABC-234 · Intervention 18 Jul          │
│                   │ Via …, Torino, IT                 (links do not change this snapshot)      │
│                   │                                                                          │
│                   │ Description                  Qty   Unit   Price              Total         │
│                   │ Front brake discs            2     each   €70                €140          │
│                   │ Brake replacement labour     2     hour   €50                €100          │
│                   │                                               Total          €240          │
│                   │                                               Paid           €160          │
│                   │                                               Outstanding     €80          │
│                   │ Payments                                                                  │
│                   │ 18 Jul · €160 · Card · recorded by Filippo                               │
│                   │ Export unavailable until backend support is implemented.                  │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Invoice 2026-00012           │
├──────────────────────────────┤
│ ISSUED · PARTIALLY PAID      │
│ Mario Rossi                  │
│ Issued 18 Jul · Due 1 Aug    │
│ [ Record payment          ]  │
│ [Actions ▾]                  │
│                              │
│ Bill to snapshot             │
│ Mario Rossi · Via …, Torino  │
│ Related: 1-ABC-234 →         │
│                              │
│ Invoice lines                │
│ Front brake discs     €140   │
│ Labour                €100   │
│ Total                  €240  │
│ Paid                   €160  │
│ Outstanding             €80  │
│                              │
│ Payments                     │
│ 18 Jul · Card · €160         │
│ Recorded by Filippo          │
│                              │
│ Export unavailable           │
├──────────────────────────────┤
│ Home   Vehicles   Jobs  More │
└──────────────────────────────┘
```

When fully paid, the primary payment action is removed and the Paid badge plus zero balance are
shown. Unpaid shows paid amount zero. Void shows the retained number, void timestamp/reason, and no
payment action.

## Record payment

| Field | Rule |
| --- | --- |
| Amount | Required, positive, same displayed currency, no greater than current outstanding balance |
| Received at | Required date/time according to backend contract |
| Method | Cash, bank transfer, card, or other |
| Reference | Optional bounded text |
| Notes | Optional bounded text |

### Desktop dialog and phone sheet

```text
┌────────────────────────────────────────────┐  ┌──────────────────────────────┐
│ Record payment                         [×] │  │ Record payment          [×] │
│ Invoice 2026-00012 · Outstanding €80      │  │ 2026-00012 · Due €80       │
│ Payment records cannot be edited/deleted.  │  │ Cannot be edited/deleted.  │
│ Amount (required) [________] EUR           │  │ Amount [________] EUR       │
│ Received (required) [____________]         │  │ Received [______________]  │
│ Method [Card ▾]                            │  │ Method [Card ▾]            │
│ Reference [____________________________]   │  │ Reference [_____________]  │
│ Notes [________________________________]   │  │ Notes [_________________]  │
│ [Cancel]                 [Record payment]  │  │ [ Record payment        ]  │
└────────────────────────────────────────────┘  └──────────────────────────────┘
```

Success closes the form, refreshes payments and all derived amounts/status, and announces the new
balance. Concurrent overpayment `409` keeps non-sensitive fields, displays the new authoritative
balance, and requires explicit resubmission with a valid amount. No Edit or Delete appears on a
payment row.

## Void flow

Void is offered only when the backend says the issued invoice is eligible. The confirmation
requires a bounded reason and repeats the invoice number and total. It explicitly states that the
invoice stays in records and cannot receive payments. Paid invoices do not show Void; a stale
attempt receives a conflict and reloads payment status.

## State and error coverage

- `422`: customer/relationship, dates, line, issue, void-reason, and payment fields retain input.
- `409`: relationship change, concurrent issue/void/payment, currency mismatch, overpayment, or
  immutable lifecycle; show Reload latest and authoritative totals/status.
- `404`: invoice not found → Invoices; missing selected relation → correct draft selection.
- Expired session: clear stale session and redirect to login with safe invoice path.
- `503`: preserve draft form input where safe; payment result remains unknown until authoritative
  invoice reload, so do not show success or invite an immediate duplicate submission.
- Unexpected errors display a correlation reference and never expose number-sequence internals.


