DEFINE TABLE IF NOT EXISTS user SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS email ON user TYPE string
    ASSERT string::len($value) > 0 AND string::len($value) <= 254;

DEFINE FIELD IF NOT EXISTS email_normalized ON user TYPE string
    ASSERT string::len($value) > 0 AND string::len($value) <= 254;

DEFINE FIELD IF NOT EXISTS display_name ON user TYPE string
    ASSERT string::len($value) > 0 AND string::len($value) <= 120;

DEFINE FIELD IF NOT EXISTS password_hash ON user TYPE string
    ASSERT string::starts_with($value, '$argon2id$');

DEFINE FIELD IF NOT EXISTS active ON user TYPE bool DEFAULT true;

DEFINE FIELD IF NOT EXISTS created_at ON user TYPE datetime DEFAULT time::now() READONLY;

DEFINE FIELD IF NOT EXISTS updated_at ON user TYPE datetime DEFAULT ALWAYS time::now();

DEFINE INDEX IF NOT EXISTS user_email_normalized_unique ON user
    FIELDS email_normalized UNIQUE;
