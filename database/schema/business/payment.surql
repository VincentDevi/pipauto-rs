DEFINE TABLE IF NOT EXISTS payment SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS invoice ON payment TYPE record<invoice>
    REFERENCE ON DELETE REJECT
    ASSERT record::exists($value) AND $value.status = 'issued';

DEFINE FIELD IF NOT EXISTS amount_minor ON payment TYPE int
    ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS currency ON payment TYPE string
    ASSERT string::matches($value, /^[A-Z]{3}$/) AND $value = invoice.currency;
DEFINE FIELD IF NOT EXISTS received_at ON payment TYPE datetime;
DEFINE FIELD IF NOT EXISTS method ON payment TYPE string
    ASSERT $value IN ['cash', 'bank_transfer', 'card', 'other'];
DEFINE FIELD IF NOT EXISTS reference ON payment TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value AND string::len($value) >= 1
    );
DEFINE FIELD IF NOT EXISTS notes ON payment TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value AND string::len($value) >= 1
    );
DEFINE FIELD IF NOT EXISTS created_at ON payment TYPE datetime
    DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS created_by ON payment TYPE record<user>
    REFERENCE ON DELETE REJECT
    ASSERT record::exists($value);

DEFINE INDEX IF NOT EXISTS payment_invoice_received ON payment
    FIELDS invoice, received_at, created_at;
DEFINE INDEX IF NOT EXISTS payment_created_by ON payment FIELDS created_by;

DEFINE EVENT IF NOT EXISTS payment_validate_create ON TABLE payment
    WHEN $event = 'CREATE'
    THEN {
        IF $after.invoice.status != 'issued' {
            THROW 'payments require an issued invoice';
        };
        IF $after.currency != $after.invoice.currency {
            THROW 'payment currency must match its invoice';
        };
        LET $paid = math::sum((SELECT VALUE amount_minor FROM payment
            WHERE invoice = $after.invoice));
        IF $paid > $after.invoice.total_minor {
            THROW 'payment would exceed the invoice outstanding balance';
        };
    };

DEFINE EVENT IF NOT EXISTS payment_reject_mutation ON TABLE payment
    WHEN $event IN ['UPDATE', 'DELETE']
    THEN {
        THROW 'recorded payments are append-only';
    };
