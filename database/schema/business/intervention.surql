DEFINE TABLE IF NOT EXISTS intervention SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS vehicle ON intervention TYPE record<vehicle>
    REFERENCE ON DELETE REJECT
    ASSERT record::exists($value);

DEFINE FIELD IF NOT EXISTS service_date ON intervention TYPE datetime;

DEFINE FIELD IF NOT EXISTS status ON intervention TYPE string DEFAULT 'draft'
    ASSERT $value IN ['draft', 'completed', 'cancelled']
        AND ($before IS NOT NONE OR $value = 'draft')
        AND ($before IS NONE OR $before = 'draft' OR $value = $before)
        AND ($value != 'completed' OR performed_work IS NOT NONE);

DEFINE FIELD IF NOT EXISTS mileage ON intervention TYPE option<int>
    ASSERT $value IS NONE OR $value >= 0;

DEFINE FIELD IF NOT EXISTS customer_reported_problem ON intervention TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 10000
    );

DEFINE FIELD IF NOT EXISTS diagnostics ON intervention TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 10000
    );

DEFINE FIELD IF NOT EXISTS performed_work ON intervention TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 10000
    );

DEFINE FIELD IF NOT EXISTS recommendations ON intervention TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 10000
    );

DEFINE FIELD IF NOT EXISTS notes ON intervention TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 10000
    );

DEFINE FIELD IF NOT EXISTS currency ON intervention TYPE string DEFAULT 'EUR'
    ASSERT string::matches($value, /^[A-Z]{3}$/);

DEFINE FIELD IF NOT EXISTS created_at ON intervention TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS updated_at ON intervention TYPE datetime DEFAULT ALWAYS time::now();

DEFINE FIELD IF NOT EXISTS completed_at ON intervention TYPE option<datetime>
    ASSERT (
        status = 'completed'
        AND $value IS NOT NONE
        AND $value >= created_at
    ) OR (status != 'completed' AND $value IS NONE);

DEFINE FIELD IF NOT EXISTS cancelled_at ON intervention TYPE option<datetime>
    ASSERT (
        status = 'cancelled'
        AND $value IS NOT NONE
        AND $value >= created_at
    ) OR (status != 'cancelled' AND $value IS NONE);

DEFINE INDEX IF NOT EXISTS intervention_vehicle_service_history ON intervention
    FIELDS vehicle, service_date, created_at;
DEFINE INDEX IF NOT EXISTS intervention_status ON intervention FIELDS status;
DEFINE INDEX IF NOT EXISTS intervention_recent_work ON intervention
    FIELDS service_date, created_at;

DEFINE EVENT IF NOT EXISTS intervention_reject_terminal_update ON TABLE intervention
    WHEN $event = 'UPDATE' AND $before.status IN ['completed', 'cancelled']
    THEN {
        THROW 'completed and cancelled interventions are immutable';
    };

DEFINE EVENT IF NOT EXISTS intervention_reject_delete ON TABLE intervention
    WHEN $event = 'DELETE'
    THEN {
        THROW 'interventions are durable and cannot be deleted';
    };

DEFINE EVENT IF NOT EXISTS intervention_validate_state ON TABLE intervention
    WHEN $event IN ['CREATE', 'UPDATE']
    THEN {
        IF $after.status = 'draft'
            AND ($after.completed_at IS NOT NONE OR $after.cancelled_at IS NOT NONE) {
            THROW 'draft interventions cannot have terminal timestamps';
        };
        IF $after.status = 'completed' AND (
            $after.performed_work IS NONE
            OR $after.completed_at IS NONE
            OR $after.completed_at < $after.created_at
            OR $after.cancelled_at IS NOT NONE
        ) {
            THROW 'completed intervention state is inconsistent';
        };
        IF $after.status = 'cancelled' AND (
            $after.cancelled_at IS NONE
            OR $after.cancelled_at < $after.created_at
            OR $after.completed_at IS NOT NONE
        ) {
            THROW 'cancelled intervention state is inconsistent';
        };
    };

DEFINE EVENT IF NOT EXISTS intervention_validate_mileage ON TABLE intervention
    WHEN $event IN ['CREATE', 'UPDATE']
        AND $after.status != 'cancelled'
        AND $after.mileage IS NOT NONE
    THEN {
        LET $previous = (SELECT VALUE mileage FROM intervention
            WHERE id != $after.id
                AND vehicle = $after.vehicle
                AND status != 'cancelled'
                AND mileage IS NOT NONE
                AND (
                    service_date < $after.service_date
                    OR (service_date = $after.service_date AND created_at < $after.created_at)
                    OR (
                        service_date = $after.service_date
                        AND created_at = $after.created_at
                        AND id < $after.id
                    )
                )
            ORDER BY service_date DESC, created_at DESC, id DESC
            LIMIT 1)[0];
        LET $next = (SELECT VALUE mileage FROM intervention
            WHERE id != $after.id
                AND vehicle = $after.vehicle
                AND status != 'cancelled'
                AND mileage IS NOT NONE
                AND (
                    service_date > $after.service_date
                    OR (service_date = $after.service_date AND created_at > $after.created_at)
                    OR (
                        service_date = $after.service_date
                        AND created_at = $after.created_at
                        AND id > $after.id
                    )
                )
            ORDER BY service_date ASC, created_at ASC, id ASC
            LIMIT 1)[0];
        IF $previous IS NOT NONE AND $after.mileage < $previous {
            THROW 'intervention mileage is lower than the previous service-history mileage';
        };
        IF $next IS NOT NONE AND $after.mileage > $next {
            THROW 'intervention mileage is higher than the next service-history mileage';
        };
    };
