DEFINE TABLE IF NOT EXISTS login_throttle SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS identifier_digest ON login_throttle TYPE string
    ASSERT string::len($value) = 43;

DEFINE FIELD IF NOT EXISTS network_digest ON login_throttle TYPE string
    ASSERT string::len($value) = 43;

DEFINE FIELD IF NOT EXISTS failed_attempts ON login_throttle TYPE int DEFAULT 0;

DEFINE FIELD IF NOT EXISTS window_started_at ON login_throttle TYPE datetime;

DEFINE FIELD IF NOT EXISTS blocked_until ON login_throttle TYPE option<datetime>;

DEFINE INDEX IF NOT EXISTS login_throttle_key_unique ON login_throttle
    FIELDS identifier_digest, network_digest UNIQUE;

DEFINE INDEX IF NOT EXISTS login_throttle_blocked_until ON login_throttle FIELDS blocked_until;
