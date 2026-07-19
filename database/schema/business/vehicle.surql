DEFINE TABLE IF NOT EXISTS vehicle SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS customer ON vehicle TYPE record<customer>
    REFERENCE ON DELETE REJECT
    ASSERT record::exists($value) AND $value.archived_at IS NONE;

DEFINE FIELD IF NOT EXISTS make ON vehicle TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 80;

DEFINE FIELD IF NOT EXISTS make_normalized ON vehicle TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 240;

DEFINE FIELD IF NOT EXISTS model ON vehicle TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 80;

DEFINE FIELD IF NOT EXISTS model_normalized ON vehicle TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 240;

DEFINE FIELD IF NOT EXISTS year ON vehicle TYPE option<int>
    ASSERT $value IS NONE OR ($value >= 1886 AND $value <= time::year() + 1);

DEFINE FIELD IF NOT EXISTS registration ON vehicle TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 40
    );

DEFINE FIELD IF NOT EXISTS registration_normalized ON vehicle TYPE option<string>
    ASSERT $value IS NONE OR string::matches($value, /^[A-Z0-9]{2,16}$/);

DEFINE FIELD IF NOT EXISTS vin ON vehicle TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 40
    );

DEFINE FIELD IF NOT EXISTS vin_normalized ON vehicle TYPE option<string>
    ASSERT $value IS NONE OR string::matches($value, /^[A-HJ-NPR-Z0-9]{17}$/);

DEFINE FIELD IF NOT EXISTS current_mileage ON vehicle TYPE option<int>
    ASSERT $value IS NONE OR $value >= 0;

DEFINE FIELD IF NOT EXISTS engine_type ON vehicle TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 160
    );

DEFINE FIELD IF NOT EXISTS notes ON vehicle TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 10000
    );

DEFINE FIELD IF NOT EXISTS created_at ON vehicle TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS updated_at ON vehicle TYPE datetime DEFAULT ALWAYS time::now();
DEFINE FIELD IF NOT EXISTS archived_at ON vehicle TYPE option<datetime>;

DEFINE INDEX IF NOT EXISTS vehicle_customer ON vehicle FIELDS customer;
DEFINE INDEX IF NOT EXISTS vehicle_registration_normalized_unique ON vehicle
    FIELDS registration_normalized UNIQUE;
DEFINE INDEX IF NOT EXISTS vehicle_vin_normalized_unique ON vehicle FIELDS vin_normalized UNIQUE;
DEFINE INDEX IF NOT EXISTS vehicle_make_model_normalized ON vehicle
    FIELDS make_normalized, model_normalized;
DEFINE INDEX IF NOT EXISTS vehicle_archived_at ON vehicle FIELDS archived_at;
