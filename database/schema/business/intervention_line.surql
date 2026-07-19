DEFINE TABLE IF NOT EXISTS intervention_line SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS intervention ON intervention_line TYPE record<intervention>
    REFERENCE ON DELETE REJECT
    ASSERT record::exists($value) AND $value.status = 'draft';

DEFINE FIELD IF NOT EXISTS category ON intervention_line TYPE string
    ASSERT $value IN ['labour', 'part', 'material', 'other'];

DEFINE FIELD IF NOT EXISTS description ON intervention_line TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 500;

DEFINE FIELD IF NOT EXISTS quantity ON intervention_line TYPE decimal
    ASSERT $value > 0dec AND $value = math::round($value * 1000dec) / 1000dec;

DEFINE FIELD IF NOT EXISTS unit_label ON intervention_line TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 32;

DEFINE FIELD IF NOT EXISTS currency ON intervention_line TYPE string
    ASSERT string::matches($value, /^[A-Z]{3}$/) AND $value = intervention.currency;

DEFINE FIELD IF NOT EXISTS unit_price_minor ON intervention_line TYPE int
    ASSERT $value >= 0;

DEFINE FIELD IF NOT EXISTS unit_cost_minor ON intervention_line TYPE option<int>
    ASSERT $value IS NONE OR $value >= 0;

DEFINE FIELD IF NOT EXISTS total_price_minor ON intervention_line TYPE int
    ASSERT $value >= 0 AND $value = math::round(quantity * unit_price_minor);

DEFINE FIELD IF NOT EXISTS total_cost_minor ON intervention_line TYPE option<int>
    ASSERT (
        unit_cost_minor IS NONE
        AND $value IS NONE
    ) OR (
        unit_cost_minor IS NOT NONE
        AND $value IS NOT NONE
        AND $value >= 0
        AND $value = math::round(quantity * unit_cost_minor)
    );

DEFINE FIELD IF NOT EXISTS position ON intervention_line TYPE int
    ASSERT $value >= 0;

DEFINE FIELD IF NOT EXISTS created_at ON intervention_line TYPE datetime
    DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS updated_at ON intervention_line TYPE datetime
    DEFAULT ALWAYS time::now();

DEFINE INDEX IF NOT EXISTS intervention_line_position_unique ON intervention_line
    FIELDS intervention, position UNIQUE;
DEFINE INDEX IF NOT EXISTS intervention_line_intervention ON intervention_line
    FIELDS intervention;

DEFINE EVENT IF NOT EXISTS intervention_line_reject_terminal_mutation ON TABLE intervention_line
    WHEN (
        $event = 'UPDATE'
        AND $before.intervention.status IN ['completed', 'cancelled']
    ) OR (
        $event = 'DELETE'
        AND $before.intervention.status IN ['completed', 'cancelled']
    )
    THEN {
        THROW 'completed and cancelled intervention lines are immutable';
    };

DEFINE EVENT IF NOT EXISTS intervention_line_validate_totals ON TABLE intervention_line
    WHEN $event IN ['CREATE', 'UPDATE']
    THEN {
        IF $after.currency != $after.intervention.currency {
            THROW 'intervention line currency must match its intervention';
        };
        IF $after.total_price_minor != math::round(
            $after.quantity * $after.unit_price_minor
        ) {
            THROW 'intervention line price total is inconsistent';
        };
        IF (
            $after.unit_cost_minor IS NONE
            AND $after.total_cost_minor IS NOT NONE
        ) OR (
            $after.unit_cost_minor IS NOT NONE
            AND (
                $after.total_cost_minor IS NONE
                OR $after.total_cost_minor != math::round(
                    $after.quantity * $after.unit_cost_minor
                )
            )
        ) {
            THROW 'intervention line cost total is inconsistent';
        };
    };

DEFINE EVENT IF NOT EXISTS intervention_reject_line_currency_mismatch ON TABLE intervention
    WHEN $event = 'UPDATE' AND $before.currency != $after.currency
    THEN {
        LET $mismatched_lines = count(SELECT * FROM intervention_line
            WHERE intervention = $after.id AND currency != $after.currency);
        IF $mismatched_lines > 0 {
            THROW 'intervention currency cannot conflict with existing lines';
        };
    };
