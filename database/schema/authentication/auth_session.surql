DEFINE TABLE IF NOT EXISTS auth_session SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS user ON auth_session TYPE record<user>;

DEFINE FIELD IF NOT EXISTS jti_digest ON auth_session TYPE string;

DEFINE FIELD IF NOT EXISTS issued_at ON auth_session TYPE datetime;

DEFINE FIELD IF NOT EXISTS expires_at ON auth_session TYPE datetime;

DEFINE FIELD IF NOT EXISTS revoked_at ON auth_session TYPE option<datetime>;

DEFINE FIELD IF NOT EXISTS last_seen_at ON auth_session TYPE option<datetime>;

DEFINE FIELD IF NOT EXISTS created_ip_digest ON auth_session TYPE option<string>
    ASSERT $value IS NONE OR string::len($value) = 43;

DEFINE FIELD IF NOT EXISTS user_agent_summary ON auth_session TYPE option<string>
    ASSERT $value IS NONE OR (
        string::len($value) <= 256 AND !$value.contains('\\n') AND !$value.contains('\\r')
    );

DEFINE INDEX IF NOT EXISTS auth_session_jti_unique ON auth_session FIELDS jti_digest UNIQUE;

DEFINE INDEX IF NOT EXISTS auth_session_user ON auth_session FIELDS user;

DEFINE INDEX IF NOT EXISTS auth_session_expires_at ON auth_session FIELDS expires_at;
