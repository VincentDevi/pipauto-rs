DEFINE TABLE IF NOT EXISTS customer SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS display_name ON customer TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 160;

DEFINE FIELD IF NOT EXISTS display_name_normalized ON customer TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 480;

DEFINE FIELD IF NOT EXISTS email ON customer TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 3
        AND string::len($value) <= 254
        AND string::matches($value, /^[^\s@]+@[^\s@]+\.[^\s@]+$/)
    );

DEFINE FIELD IF NOT EXISTS email_normalized ON customer TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 3
        AND string::len($value) <= 254
        AND $value = string::lowercase($value)
        AND string::matches($value, /^[^\s@]+@[^\s@]+\.[^\s@]+$/)
    );

DEFINE FIELD IF NOT EXISTS phone ON customer TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 40
    );

DEFINE FIELD IF NOT EXISTS phone_normalized ON customer TYPE option<string>
    ASSERT $value IS NONE OR string::matches($value, /^\+?[0-9]+$/);

DEFINE FIELD IF NOT EXISTS address ON customer TYPE option<object>
    ASSERT $value IS NONE OR (
        $value.line_1 IS NOT NONE
        AND $value.postal_code IS NOT NONE
        AND $value.city IS NOT NONE
        AND $value.country_code IS NOT NONE
    );

DEFINE FIELD IF NOT EXISTS address.line_1 ON customer TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 160
    );

DEFINE FIELD IF NOT EXISTS address.line_2 ON customer TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 160
    );

DEFINE FIELD IF NOT EXISTS address.postal_code ON customer TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 32
    );

DEFINE FIELD IF NOT EXISTS address.city ON customer TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 120
    );

DEFINE FIELD IF NOT EXISTS address.country_code ON customer TYPE option<string>
    ASSERT $value IS NONE OR string::matches($value, /^[A-Z]{2}$/);

DEFINE FIELD IF NOT EXISTS notes ON customer TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 10000
    );

DEFINE FIELD IF NOT EXISTS created_at ON customer TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS updated_at ON customer TYPE datetime DEFAULT ALWAYS time::now();
DEFINE FIELD IF NOT EXISTS archived_at ON customer TYPE option<datetime>;

DEFINE INDEX IF NOT EXISTS customer_display_name_normalized ON customer
    FIELDS display_name_normalized;
DEFINE INDEX IF NOT EXISTS customer_email_normalized ON customer FIELDS email_normalized;
DEFINE INDEX IF NOT EXISTS customer_phone_normalized ON customer FIELDS phone_normalized;
DEFINE INDEX IF NOT EXISTS customer_archived_at ON customer FIELDS archived_at;
