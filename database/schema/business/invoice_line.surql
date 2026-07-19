DEFINE TABLE IF NOT EXISTS invoice_line SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS invoice ON invoice_line TYPE record<invoice>
    REFERENCE ON DELETE REJECT
    ASSERT record::exists($value) AND $value.status = 'draft';

DEFINE FIELD IF NOT EXISTS source_intervention_line ON invoice_line
    TYPE option<record<intervention_line>>
    REFERENCE ON DELETE REJECT
    ASSERT $value IS NONE OR record::exists($value);

DEFINE FIELD IF NOT EXISTS description ON invoice_line TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 500;

DEFINE FIELD IF NOT EXISTS quantity ON invoice_line TYPE decimal
    ASSERT $value > 0dec AND $value = math::round($value * 1000dec) / 1000dec;

DEFINE FIELD IF NOT EXISTS unit_label ON invoice_line TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 32;

DEFINE FIELD IF NOT EXISTS currency ON invoice_line TYPE string
    ASSERT string::matches($value, /^[A-Z]{3}$/) AND $value = invoice.currency;

DEFINE FIELD IF NOT EXISTS unit_price_minor ON invoice_line TYPE int
    ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS line_total_minor ON invoice_line TYPE int
    ASSERT $value >= 0 AND $value = math::round(quantity * unit_price_minor);
DEFINE FIELD IF NOT EXISTS position ON invoice_line TYPE int
    ASSERT $value >= 0;

DEFINE FIELD IF NOT EXISTS created_at ON invoice_line TYPE datetime
    DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS updated_at ON invoice_line TYPE datetime
    DEFAULT ALWAYS time::now();

DEFINE INDEX IF NOT EXISTS invoice_line_position_unique ON invoice_line
    FIELDS invoice, position UNIQUE;
DEFINE INDEX IF NOT EXISTS invoice_line_invoice ON invoice_line FIELDS invoice;
DEFINE INDEX IF NOT EXISTS invoice_line_source_intervention_line ON invoice_line
    FIELDS source_intervention_line;

DEFINE EVENT IF NOT EXISTS invoice_line_reject_terminal_mutation ON TABLE invoice_line
    WHEN (
        $event = 'UPDATE' AND $before.invoice.status IN ['issued', 'void']
    ) OR (
        $event = 'DELETE' AND $before.invoice.status IN ['issued', 'void']
    )
    THEN {
        THROW 'issued and void invoice lines are immutable';
    };

DEFINE EVENT IF NOT EXISTS invoice_line_validate_snapshot ON TABLE invoice_line
    WHEN $event IN ['CREATE', 'UPDATE']
    THEN {
        IF $after.invoice.status != 'draft' {
            THROW 'invoice lines are editable only while the invoice is a draft';
        };
        IF $after.currency != $after.invoice.currency {
            THROW 'invoice line currency must match its invoice';
        };
        IF $after.line_total_minor != math::round(
            $after.quantity * $after.unit_price_minor
        ) {
            THROW 'invoice line total is inconsistent';
        };
    };

DEFINE EVENT IF NOT EXISTS invoice_reject_line_currency_mismatch ON TABLE invoice
    WHEN $event = 'UPDATE' AND $before.currency != $after.currency
    THEN {
        LET $mismatched_lines = count(SELECT * FROM invoice_line
            WHERE invoice = $after.id AND currency != $after.currency);
        IF $mismatched_lines > 0 {
            THROW 'invoice currency cannot conflict with existing lines';
        };
    };
