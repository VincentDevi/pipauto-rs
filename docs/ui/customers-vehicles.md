# Customers and vehicles

## Customers list

| Property | Specification |
| --- | --- |
| Route | `GET /customers` |
| Access | Authenticated |
| Entry/exit | Sidebar/More, dashboard quick action; opens customer detail or new customer |
| Filters | Text query and Active/Archived state |
| Result data | Display name, phone, email, city, archive state; vehicle count only if supplied without a new query contract |
| Primary action | New customer → `/customers/new` |
| Backend | Customer list/search with opaque cursor; archive filter; no duplicate merging |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Customers                                          [ + New customer ]   │
│ ▌ Customers       │ Find a customer by name, email, or phone.                                 │
│                   │ [Search________________________________] [Active ▾] [ Search ]             │
│                   │                                                                          │
│                   │ Name              Phone          Email             City       Status      │
│                   │ Mario Rossi       +39 …          mario@…           Torino     Active      │
│                   │ Giulia Bianchi    +39 …          —                 Milano     Active      │
│                   │                                                                          │
│                   │ [Previous]                                           [Next]              │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Customers                    │
├──────────────────────────────┤
│ [ + New customer           ] │
│ [Search customers_________]  │
│ [ Filters (Active)         ] │
│                              │
│ ┌──────────────────────────┐ │
│ │ Mario Rossi       ACTIVE │ │
│ │ +39 …                    │ │
│ │ mario@example.com        │ │
│ │ Torino                   │ │
│ └──────────────────────────┘ │
│ ┌──────────────────────────┐ │
│ │ Giulia Bianchi    ACTIVE │ │
│ │ +39 … · Milano           │ │
│ └──────────────────────────┘ │
│ [Previous]          [Next]   │
├──────────────────────────────┤
│Home Vehicles Calendar Jobs More│
└──────────────────────────────┘
```

No results repeats the active query and offers Clear search. An empty database offers New customer.
A failed search leaves filters visible and replaces only results with Retry.

## Create and edit customer

| Property | Specification |
| --- | --- |
| Routes | `GET /customers/new`, `GET /customers/{id}/edit` |
| Required | Display name, trimmed, 1–160 characters |
| Optional | Email, phone, address line 1/2, postal code, city, two-letter country, workshop notes |
| Actions | Save customer; Cancel to customer detail or list |
| Backend | Create/update validation; normalized search fields remain server-owned |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Customers / New customer                                                  │
│                   │ New customer                                                               │
│                   │ [error summary]                                                           │
│                   │ ┌────────────────────────────┐ ┌─────────────────────────────────────────┐ │
│                   │ │ Display name (required)    │ │ Contact                                 │ │
│                   │ │ [________________________] │ │ Email [______________________________] │ │
│                   │ │ Workshop notes             │ │ Phone [______________________________] │ │
│                   │ │ [________________________] │ └─────────────────────────────────────────┘ │
│                   │ │ [________________________] │ ┌─────────────────────────────────────────┐ │
│                   │ └────────────────────────────┘ │ Address                                 │ │
│                   │                                │ Line 1 / Line 2 / Postal / City / Country│ │
│                   │                                └─────────────────────────────────────────┘ │
│                   │ [Cancel]                                           [ Save customer ]     │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ New customer                 │
├──────────────────────────────┤
│ [error summary]              │
│ Display name (required)      │
│ [__________________________] │
│ Email                        │
│ [__________________________] │
│ Phone                        │
│ [__________________________] │
│ Address line 1               │
│ [__________________________] │
│ Address line 2               │
│ [__________________________] │
│ Postal code / City           │
│ [________] [______________]  │
│ Country [__]                 │
│ Workshop notes               │
│ [__________________________] │
│ [ Save customer            ] │
│ Cancel                       │
└──────────────────────────────┘
```

Edit uses the same layout, title **Edit customer**, and button **Save changes**. `422` responses
preserve every field. `404` uses the customer-not-found page. Successful create/edit goes to detail.

## Customer detail

| Property | Specification |
| --- | --- |
| Route | `GET /customers/{id}` |
| Data | Contact, address, notes, status, timestamps where useful, current vehicles |
| Primary action | Register vehicle → `/customers/{id}/vehicles/new` when active |
| Secondary actions | Edit; Archive or Restore; open vehicle |
| Restrictions | Archived customer cannot receive a new/reassigned vehicle |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Customers / Mario Rossi                                                   │
│                   │ Mario Rossi  [ACTIVE]                   [Register vehicle] [Actions ▾]     │
│                   │                                                                          │
│                   │ Contact                       Address                                     │
│                   │ +39 … · mario@example.com     Via …, 10100 Torino, IT                     │
│                   │ Workshop notes: Prefers phone contact.                                    │
│                   │                                                                          │
│                   │ Vehicles                                                                 │
│                   │ Registration  Vehicle           Year  Mileage   Engine          Status    │
│                   │ 1-ABC-234     Volkswagen Golf   2018  126,400   2.0 TDI         Active    │
│                   │ [Previous]                                                     [Next]     │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Customers / Mario Rossi      │
├──────────────────────────────┤
│ Mario Rossi           ACTIVE │
│ [ Register vehicle         ] │
│ [Edit] [Actions ▾]           │
│                              │
│ Contact                      │
│ +39 …                        │
│ mario@example.com            │
│                              │
│ Address                      │
│ Via …, 10100 Torino, IT      │
│                              │
│ Vehicles                     │
│ ┌──────────────────────────┐ │
│ │ 1-ABC-234 · ACTIVE       │ │
│ │ Volkswagen Golf · 2018   │ │
│ │ 126,400 km · 2.0 TDI     │ │
│ └──────────────────────────┘ │
├──────────────────────────────┤
│Home Vehicles Calendar Jobs More│
└──────────────────────────────┘
```

An archived detail replaces Register vehicle with an Archived explanation and Restore. Archive
confirmation states that vehicles/history remain and the customer leaves active lists. No vehicles
offers Register vehicle when active; archived customers get a read-only empty message.

## Vehicles list

| Property | Specification |
| --- | --- |
| Route | `GET /vehicles` |
| Filters | Text, registration, VIN, make, model, customer, Active/Archived |
| Result data | Registration or “No registration”, make/model/year, current customer, mileage, status |
| Primary action | Register vehicle; select an active customer before the nested form |
| Backend | Vehicle list/search, exact normalized identifier lookup, opaque cursor |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Vehicles                                           [ + Register vehicle ]│
│ ▌ Vehicles        │ [Search registration, VIN, make, model___________] [ Search ]             │
│                   │ [More filters: Customer · Make · Model · Active]                         │
│                   │                                                                          │
│                   │ Registration  Vehicle             Customer       Mileage    Status        │
│                   │ 1-ABC-234     VW Golf · 2018      Mario Rossi    126,400 km Active        │
│                   │ No reg.       Fiat Panda · 2009   G. Bianchi     88,200 km  Active        │
│                   │ [Previous]                                               [Next]           │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Vehicles                     │
├──────────────────────────────┤
│ [ + Register vehicle       ] │
│ [Registration, VIN, make__]  │
│ [ Filters (1)              ] │
│                              │
│ ┌──────────────────────────┐ │
│ │ 1-ABC-234        ACTIVE  │ │
│ │ Volkswagen Golf · 2018   │ │
│ │ Mario Rossi              │ │
│ │ 126,400 km · 2.0 TDI     │ │
│ └──────────────────────────┘ │
│ [Previous]          [Next]   │
├──────────────────────────────┤
│Home Vehicles Calendar Jobs More│
└──────────────────────────────┘
```

## Register and edit vehicle

| Property | Specification |
| --- | --- |
| Routes | `GET /customers/{id}/vehicles/new`, `GET /vehicles/{id}/edit` |
| Required | Active customer, make, model |
| Optional | Year, registration, VIN, current mileage, engine type, notes |
| Validation | Year no later than next year; valid VIN when present; non-negative mileage; unique normalized VIN/registration |
| Actions | Save vehicle; edit may open Reassign owner; Cancel |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Vehicles / Register vehicle                                               │
│                   │ Register vehicle for Mario Rossi                                           │
│                   │ [error summary or identifier conflict]                                    │
│                   │ Make (required) [________________]  Model (required) [__________________]  │
│                   │ Year [____]  Registration [____________]  VIN [________________________]  │
│                   │ Current mileage [________] km       Engine type [_______________________]  │
│                   │ Notes [________________________________________________________________]  │
│                   │ [Cancel]                                            [ Save vehicle ]     │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Register vehicle             │
├──────────────────────────────┤
│ Owner                        │
│ Mario Rossi                  │
│ [error summary]              │
│ Make (required)              │
│ [__________________________] │
│ Model (required)             │
│ [__________________________] │
│ Year [______]                │
│ Registration                 │
│ [__________________________] │
│ VIN                          │
│ [__________________________] │
│ Current mileage [_______] km │
│ Engine type                  │
│ [__________________________] │
│ Notes [____________________] │
│ [ Save vehicle             ] │
│ Cancel                       │
└──────────────────────────────┘
```

A `409` marks VIN and/or registration, preserves display values, and states the identifier is
already used without linking to or disclosing the other record. Edit shows current owner as a
separate read-only section; changing owner uses the confirmation flow below.

## Vehicle detail and service-history page

The vehicle detail default section is Overview followed by recent service history. The dedicated
`/vehicles/{id}/history` route focuses the full paginated history and is the target of **View complete
history**. Both share the same vehicle identity header.

| Property | Specification |
| --- | --- |
| Data | Registration, make/model/year, VIN, current mileage, engine, owner, notes, state; chronological interventions |
| Primary action | New intervention when active |
| Secondary | Edit, Reassign owner, Archive/Restore, View complete history |
| Attachment action | Upload/edit/delete while active; Open/Download remain available when archived |
| Backend | Vehicle detail, customer link, history list, stored-attachment service |

### Desktop wireframe

```text
┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
│ …navigation…      │ Vehicles / 1-ABC-234                                                     │
│                   │ Volkswagen Golf · 2018 [ACTIVE]       [New intervention] [Actions ▾]      │
│                   │ Owner: Mario Rossi →     Current mileage: 126,400 km                      │
│                   │ VIN: WVW…                 Engine: 2.0 TDI                                 │
│                   │                                                                          │
│                   │ Service history                                  [View complete history] │
│                   │ Date       Mileage    Work                         Status       Total      │
│                   │ 18 Jul     126,400    Front brake replacement      COMPLETED    €240       │
│                   │ 03 May     120,100    Annual service               COMPLETED    €180       │
│                   │ 10 Feb     118,000    Engine noise inspection      CANCELLED    —          │
│                   │                                                                          │
│                   │ Notes                                  Attachments                        │
│                   │ Customer reports intermittent…         inspection-photo.jpg · JPEG · 2 MiB│
│                   │                                        [Open] [Download] [Upload]         │
└───────────────────┴──────────────────────────────────────────────────────────────────────────┘
```

### Phone wireframe

```text
┌──────────────────────────────┐
│ Vehicles / 1-ABC-234         │
├──────────────────────────────┤
│ Volkswagen Golf · 2018       │
│ ACTIVE                       │
│ [ + New intervention       ] │
│ [Edit] [Actions ▾]           │
│                              │
│ Owner  Mario Rossi →         │
│ Mileage  126,400 km          │
│ VIN  WVW…                    │
│ Engine  2.0 TDI              │
│                              │
│ Service history              │
│ ┌──────────────────────────┐ │
│ │ COMPLETED · 18 Jul 2026  │ │
│ │ 126,400 km · €240        │ │
│ │ Front brake replacement  │ │
│ └──────────────────────────┘ │
│ View complete history →      │
│                              │
│ Attachments                  │
│ inspection-photo.jpg · JPEG  │
│ [Open] [Download] [Upload]   │
├──────────────────────────────┤
│Home Vehicles Calendar Jobs More│
└──────────────────────────────┘
```

History empty state says no interventions have been recorded and offers New intervention for an
active vehicle. Archived vehicles show history and readable attachments but disable new work and
attachment mutation.

### Reassign owner confirmation

The action opens a searchable active-customer selector. The confirmation repeats vehicle identity,
old owner, and new owner, and states that service history and invoice snapshots remain unchanged.
Archived target conflicts keep the vehicle with its current owner and reload target availability.

### Stored-attachment form

Upload requires one file and accepts optional display name and caption. The server detects media
type and byte size from the file and rejects unsupported, empty, malformed, spoofed, or oversized
content. The owner is fixed. Existing display name/caption can be edited and the attachment can be
deleted only while the vehicle is active; Open and Download remain available after archiving.
