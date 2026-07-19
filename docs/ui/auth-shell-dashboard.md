  # Authentication, application shell, and dashboard
  
  ## Shared shell
  
  All workshop pages use the same authenticated shell. Desktop keeps navigation visible; phone keeps
  record context in the header and primary destinations in the bottom bar.
  
  ### Desktop shell
  
  ```text
  ┌──────────────────────────────────────────────────────────────────────────────────────────────┐
  │ Skip to content                                                                             │
  ├───────────────────┬──────────────────────────────────────────────────────────────────────────┤
  │ PIPAUTO           │ Breadcrumb / current area                         Filippo ▾              │
  │                   ├──────────────────────────────────────────────────────────────────────────┤
  │ ▌ Dashboard       │                                                                          │
  │   Customers       │                         page content                                     │
  │   Vehicles        │                                                                          │
  │   Interventions   │                                                                          │
  │   Knowledge       │                                                                          │
  │   Invoices        │                                                                          │
  │                   │                                                                          │
  │ Filippo           │                                                                          │
  │ Sign out          │                                                                          │
  └───────────────────┴──────────────────────────────────────────────────────────────────────────┘
  ```
  
  ### Phone shell
  
  ```text
  ┌──────────────────────────────┐
  │ PIPAUTO          Page title  │
  ├──────────────────────────────┤
  │                              │
  │         page content         │
  │                              │
  │                              │
  ├──────────────────────────────┤
  │ Home   Vehicles   Jobs  More │
  └──────────────────────────────┘
  ```
  
  The desktop account control may open a small menu containing the display name and Sign out. The
  phone More sheet contains Customers, Knowledge, Invoices, the display name, and Sign out. Sign out
  is a CSRF-protected POST, never a GET link.
  
  ## Login
  
  | Property | Specification |
  | --- | --- |
  | Route | `GET /login`; existing login submission route |
  | Access | Guest-only; authenticated users go to `/` |
  | Entry | Protected-route redirect, direct link, or successful logout |
  | Exit | Safe local `next` after success; `/` by default |
  | Data | Email, password, hidden login CSRF, hidden safe `next` |
  | Actions | Sign in only; no registration, recovery, remember-me, or social login |
  | Backend | Existing authentication service, generic credential failure, throttle, unavailable response |
  
  ### Desktop wireframe
  
  ```text
  ┌──────────────────────────────────────────────────────────────────────────────┐
  │ PIPAUTO                                                                      │
  ├──────────────────────────────────────────────────────────────────────────────┤
  │                                                                              │
  │             ┌──────────────────────────────────────────────────┐             │
  │             │ Sign in to Pipauto                               │             │
  │             │ Access customer and workshop records.            │             │
  │             │                                                  │             │
  │             │ [error summary when present]                     │             │
  │             │ Email (required)                                 │             │
  │             │ [filippo@example.com__________________________]  │             │
  │             │ Password (required)                              │             │
  │             │ [••••••••••••________________________________]  │             │
  │             │                                                  │             │
  │             │ [ Sign in                                     ] │             │
  │             └──────────────────────────────────────────────────┘             │
  └──────────────────────────────────────────────────────────────────────────────┘
  ```
  
  ### Phone wireframe
  
  ```text
  ┌──────────────────────────────┐
  │ PIPAUTO                      │
  ├──────────────────────────────┤
  │ Sign in to Pipauto           │
  │ Access workshop records.     │
  │                              │
  │ [error summary]              │
  │ Email (required)             │
  │ [__________________________] │
  │ Password (required)          │
  │ [__________________________] │
  │                              │
  │ [ Sign in                  ] │
  └──────────────────────────────┘
  ```
  
  Validation keeps the normalized-safe email display value and clears the password. Invalid email or
  missing password returns `422`; bad credentials use the same generic message for unknown and known
  accounts; throttle includes a calm wait message. Loading disables Sign in and shows **Signing in…**.
  
  ## Authentication unavailable and expired session
  
  Authentication unavailable is a complete page for a standard request and a bounded fragment for a
  login-form HTMX failure. It contains the safe message, correlation reference, and **Try again**.
  Expired/stale sessions clear the cookie and redirect to Login; they do not render an error inside a
  private page.
  
  ```text
  Desktop                                      Phone
  ┌─────────────────────────────────────────┐  ┌──────────────────────────────┐
  │ PIPAUTO                                 │  │ PIPAUTO                      │
  │ ┌─────────────────────────────────────┐ │  ├──────────────────────────────┤
  │ │ Sign-in temporarily unavailable     │ │  │ Sign-in temporarily         │
  │ │ We could not confirm your sign-in.  │ │  │ unavailable                  │
  │ │ Reference: auth-…                   │ │  │ Try again shortly.           │
  │ │ [ Try again ]                       │ │  │ Reference: auth-…            │
  │ └─────────────────────────────────────┘ │  │ [ Try again                ] │
  └─────────────────────────────────────────┘  └──────────────────────────────┘
  ```
  
  ## Dashboard
  
  | Property | Specification |
  | --- | --- |
  | Route | `GET /` |
  | Access | Authenticated |
  | Entry | Login success, Home navigation, brand link |
  | Exit | All primary areas and quick-create flows |
  | Data | Recent interventions, draft interventions, outstanding issued invoices when existing collection capabilities supply them |
  | Primary action | New intervention; first select an active vehicle if not already in vehicle context |
  | Secondary actions | New customer, Register vehicle, New invoice, New technical note |
  | Backend | Existing filtered/list capabilities only; no analytics, revenue aggregation, or invented counts |
  
  If the API cannot supply a trustworthy total count, headings omit counts. Each preview is limited
  and links to its complete filtered collection.
  
  ### Desktop wireframe
  
  ```text
  ┌───────────────────┬──────────────────────────────────────────────────────────────────────────┐
  │ PIPAUTO           │ Dashboard                                      [ + New intervention ]   │
  │ ▌ Dashboard       ├──────────────────────────────────────────────────────────────────────────┤
  │   Customers       │ Good morning, Filippo                                               │
  │   Vehicles        │ Quick actions: [New customer] [Register vehicle] [New invoice]          │
  │   Interventions   │                                                                          │
  │   Knowledge       │ ┌──────────────────────────────┐ ┌─────────────────────────────────────┐ │
  │   Invoices        │ │ Draft interventions          │ │ Outstanding invoices                │ │
  │                   │ │ 18 Jul · Golf · Brake noise  │ │ 2026-00012 · Rossi · €240 unpaid   │ │
  │                   │ │ 17 Jul · Transit · Service   │ │ 2026-00011 · Bianchi · €80 due     │ │
  │                   │ │ View all drafts →            │ │ View outstanding invoices →         │ │
  │                   │ └──────────────────────────────┘ └─────────────────────────────────────┘ │
  │                   │                                                                          │
  │ Filippo           │ Recent service history                                                  │
  │ Sign out          │ Date       Vehicle       Customer      Status       Total                 │
  │                   │ 18 Jul     VW Golf       Rossi         Completed    €240                  │
  │                   │ 17 Jul     Ford Transit  Bianchi       Draft        €80                   │
  │                   │ View all interventions →                                                  │
  └───────────────────┴──────────────────────────────────────────────────────────────────────────┘
  ```
  
  ### Phone wireframe
  
  ```text
  ┌──────────────────────────────┐
  │ PIPAUTO            Dashboard │
  ├──────────────────────────────┤
  │ Good morning, Filippo        │
  │ [ + New intervention       ] │
  │                              │
  │ Quick actions                │
  │ [New customer] [Vehicle]     │
  │ [New invoice ] [Tech note]   │
  │                              │
  │ Draft interventions          │
  │ ┌──────────────────────────┐ │
  │ │ DRAFT · 18 Jul           │ │
  │ │ VW Golf · 1-ABC-234      │ │
  │ │ Brake noise              │ │
  │ └──────────────────────────┘ │
  │ View all drafts →            │
  │                              │
  │ Outstanding invoices        │
  │ ┌──────────────────────────┐ │
  │ │ UNPAID · 2026-00012      │ │
  │ │ Rossi             €240   │ │
  │ └──────────────────────────┘ │
  │ View all →                   │
  ├──────────────────────────────┤
  │ Home   Vehicles   Jobs  More │
  └──────────────────────────────┘
  ```
  
  ### Dashboard states
  
  - **Empty:** welcome text explains the first workflow; New customer is visually primary, while
    sections say no interventions/invoices exist.
  - **Partially empty:** each card has its own empty message; other cards remain usable.
  - **Loading:** server-rendered initial content; HTMX refresh keeps old content visible and marks only
    the relevant card busy.
  - **Error:** a failed card shows Retry inside that card. A complete backend failure uses the shared
    unavailable page while retaining navigation if authentication was already confirmed.
  - **Success:** create flows return to their new detail pages, not to the dashboard.
  

