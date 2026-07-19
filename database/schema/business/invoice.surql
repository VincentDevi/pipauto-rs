DEFINE SEQUENCE IF NOT EXISTS invoice_issue_number START 1;

DEFINE TABLE IF NOT EXISTS invoice SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS customer ON invoice TYPE record<customer>
    REFERENCE ON DELETE REJECT
    ASSERT record::exists($value);

DEFINE FIELD IF NOT EXISTS vehicle ON invoice TYPE option<record<vehicle>>
    REFERENCE ON DELETE REJECT
    ASSERT $value IS NONE OR record::exists($value);

DEFINE FIELD IF NOT EXISTS intervention ON invoice TYPE option<record<intervention>>
    REFERENCE ON DELETE REJECT
    ASSERT $value IS NONE OR record::exists($value);

DEFINE FIELD IF NOT EXISTS status ON invoice TYPE string DEFAULT 'draft'
    ASSERT $value IN ['draft', 'issued', 'void']
        AND ($before IS NOT NONE OR $value = 'draft')
        AND (
            $before IS NONE
            OR $value = $before
            OR ($before = 'draft' AND $value IN ['issued', 'void'])
            OR ($before = 'issued' AND $value = 'void')
        );

DEFINE FIELD IF NOT EXISTS currency ON invoice TYPE string DEFAULT 'EUR'
    ASSERT string::matches($value, /^[A-Z]{3}$/);

DEFINE FIELD IF NOT EXISTS issue_number ON invoice TYPE option<string>
    ASSERT $value IS NONE OR string::matches($value, /^[0-9]{4}-[0-9]{5,}$/);

DEFINE FIELD IF NOT EXISTS issue_date ON invoice TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS due_date ON invoice TYPE option<datetime>
    ASSERT $value IS NONE OR issue_date IS NONE OR $value >= issue_date;

DEFINE FIELD IF NOT EXISTS customer_display_snapshot ON invoice TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 160
    );

DEFINE FIELD IF NOT EXISTS billing_address_snapshot ON invoice TYPE option<object>
    ASSERT $value IS NONE OR (
        $value.line_1 IS NOT NONE
        AND $value.postal_code IS NOT NONE
        AND $value.city IS NOT NONE
        AND $value.country_code IS NOT NONE
    );

DEFINE FIELD IF NOT EXISTS billing_address_snapshot.line_1 ON invoice TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 160
    );
DEFINE FIELD IF NOT EXISTS billing_address_snapshot.line_2 ON invoice TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 160
    );
DEFINE FIELD IF NOT EXISTS billing_address_snapshot.postal_code ON invoice TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 32
    );
DEFINE FIELD IF NOT EXISTS billing_address_snapshot.city ON invoice TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 120
    );
DEFINE FIELD IF NOT EXISTS billing_address_snapshot.country_code ON invoice TYPE option<string>
    ASSERT $value IS NONE OR string::matches($value, /^[A-Z]{2}$/);

DEFINE FIELD IF NOT EXISTS notes ON invoice TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 10000
    );

DEFINE FIELD IF NOT EXISTS void_reason ON invoice TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 10000
    );

DEFINE FIELD IF NOT EXISTS subtotal_minor ON invoice TYPE int DEFAULT 0
    ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS total_minor ON invoice TYPE int DEFAULT 0
    ASSERT $value >= 0 AND $value = subtotal_minor;

DEFINE FIELD IF NOT EXISTS created_at ON invoice TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS updated_at ON invoice TYPE datetime DEFAULT ALWAYS time::now();
DEFINE FIELD IF NOT EXISTS issued_at ON invoice TYPE option<datetime>
    ASSERT $value IS NONE OR $value >= created_at;
DEFINE FIELD IF NOT EXISTS voided_at ON invoice TYPE option<datetime>
    ASSERT $value IS NONE OR $value >= created_at;

DEFINE INDEX IF NOT EXISTS invoice_issue_number_unique ON invoice FIELDS issue_number UNIQUE;
DEFINE INDEX IF NOT EXISTS invoice_customer ON invoice FIELDS customer;
DEFINE INDEX IF NOT EXISTS invoice_vehicle ON invoice FIELDS vehicle;
DEFINE INDEX IF NOT EXISTS invoice_intervention ON invoice FIELDS intervention;
DEFINE INDEX IF NOT EXISTS invoice_status ON invoice FIELDS status;
DEFINE INDEX IF NOT EXISTS invoice_issue_date ON invoice FIELDS issue_date;
DEFINE INDEX IF NOT EXISTS invoice_outstanding_listing ON invoice
    FIELDS status, due_date, issue_date;

DEFINE EVENT IF NOT EXISTS invoice_validate_state ON TABLE invoice
    WHEN $event IN ['CREATE', 'UPDATE']
    THEN {
        IF $after.subtotal_minor != $after.total_minor {
            THROW 'invoice subtotal and total must be equal while tax is outside scope';
        };
        IF $after.status = 'draft' AND (
            $after.issue_number IS NOT NONE
            OR $after.issue_date IS NOT NONE
            OR $after.issued_at IS NOT NONE
            OR $after.void_reason IS NOT NONE
            OR $after.voided_at IS NOT NONE
        ) {
            THROW 'draft invoice state is inconsistent';
        };
        IF $after.status = 'issued' AND (
            $after.issue_number IS NONE
            OR $after.issue_date IS NONE
            OR $after.issued_at IS NONE
            OR $after.customer_display_snapshot IS NONE
            OR $after.void_reason IS NOT NONE
            OR $after.voided_at IS NOT NONE
        ) {
            THROW 'issued invoice state is inconsistent';
        };
        IF $after.status = 'issued' AND !string::starts_with(
            $after.issue_number,
            <string>time::year($after.issued_at) + '-'
        ) {
            THROW 'invoice number year must match the UTC issue year';
        };
        IF $event = 'UPDATE' AND $before.status = 'draft' AND $after.status = 'issued' {
            LET $line_total = math::sum((SELECT VALUE line_total_minor FROM invoice_line
                WHERE invoice = $after.id));
            IF $line_total != $after.subtotal_minor {
                THROW 'issued invoice total must equal its persisted line snapshots';
            };
        };
        IF $after.status = 'void' AND (
            $after.void_reason IS NONE
            OR $after.voided_at IS NONE
            OR (
                $after.issued_at IS NONE
                AND (
                    $after.issue_number IS NOT NONE
                    OR $after.issue_date IS NOT NONE
                )
            )
            OR (
                $after.issued_at IS NOT NONE
                AND (
                    $after.issue_number IS NONE
                    OR $after.issue_date IS NONE
                    OR $after.customer_display_snapshot IS NONE
                )
            )
        ) {
            THROW 'void invoice state is inconsistent';
        };
        IF $after.due_date IS NOT NONE AND (
            $after.issue_date IS NONE OR $after.due_date < $after.issue_date
        ) {
            THROW 'invoice due date requires a non-later issue date';
        };
    };

DEFINE EVENT IF NOT EXISTS invoice_reject_issued_snapshot_update ON TABLE invoice
    WHEN $event = 'UPDATE'
        AND $before.status IN ['issued', 'void']
        AND (
            $before.customer != $after.customer
            OR $before.vehicle != $after.vehicle
            OR $before.intervention != $after.intervention
            OR $before.currency != $after.currency
            OR $before.issue_number != $after.issue_number
            OR $before.issue_date != $after.issue_date
            OR $before.due_date != $after.due_date
            OR $before.customer_display_snapshot != $after.customer_display_snapshot
            OR $before.billing_address_snapshot != $after.billing_address_snapshot
            OR $before.subtotal_minor != $after.subtotal_minor
            OR $before.total_minor != $after.total_minor
        )
    THEN {
        THROW 'issued invoice financial snapshots are immutable';
    };

DEFINE EVENT IF NOT EXISTS invoice_reject_number_reassignment ON TABLE invoice
    WHEN $event = 'UPDATE'
        AND $before.issue_number IS NOT NONE
        AND $before.issue_number != $after.issue_number
    THEN {
        THROW 'final invoice numbers are immutable';
    };

DEFINE EVENT IF NOT EXISTS invoice_validate_void ON TABLE invoice
    WHEN $event = 'UPDATE' AND $before.status != 'void' AND $after.status = 'void'
    THEN {
        LET $recorded = count(SELECT * FROM payment WHERE invoice = $after.id);
        IF $recorded > 0 {
            THROW 'an invoice with recorded payments cannot be voided';
        };
    };

DEFINE EVENT IF NOT EXISTS invoice_reject_delete ON TABLE invoice
    WHEN $event = 'DELETE'
    THEN {
        THROW 'invoices are durable and cannot be deleted';
    };
