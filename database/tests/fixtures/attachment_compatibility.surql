DEFINE TABLE IF NOT EXISTS attachment SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS vehicle ON attachment TYPE option<record<vehicle>>
    REFERENCE ON DELETE REJECT
    ASSERT $value IS NONE OR record::exists($value);

DEFINE FIELD IF NOT EXISTS intervention ON attachment TYPE option<record<intervention>>
    REFERENCE ON DELETE REJECT
    ASSERT $value IS NONE OR record::exists($value);

DEFINE FIELD IF NOT EXISTS technical_note ON attachment TYPE option<record<technical_note>>
    REFERENCE ON DELETE REJECT
    ASSERT $value IS NONE OR record::exists($value);

DEFINE FIELD IF NOT EXISTS display_name ON attachment TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 255;

DEFINE FIELD IF NOT EXISTS media_type ON attachment TYPE string
    ASSERT $value IN [
        'application/pdf',
        'image/heic',
        'image/heif',
        'image/jpeg',
        'image/png',
        'image/webp'
    ];

DEFINE FIELD IF NOT EXISTS byte_size ON attachment TYPE option<int>
    ASSERT $value IS NONE OR ($value >= 0 AND $value <= 26214400);

DEFINE FIELD IF NOT EXISTS caption ON attachment TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 1000
    );

DEFINE FIELD IF NOT EXISTS file ON attachment TYPE option<file<pipauto_attachments>>;

DEFINE FIELD IF NOT EXISTS sha256 ON attachment TYPE option<string>
    ASSERT $value IS NONE OR string::matches($value, /^[0-9a-f]{64}$/);

DEFINE FIELD IF NOT EXISTS storage_state ON attachment TYPE string DEFAULT 'metadata_only'
    ASSERT $value IN ['metadata_only', 'pending', 'stored', 'deleting'];

DEFINE FIELD IF NOT EXISTS created_at ON attachment TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS updated_at ON attachment TYPE datetime DEFAULT ALWAYS time::now();

DEFINE INDEX IF NOT EXISTS attachment_vehicle ON attachment FIELDS vehicle, created_at;
DEFINE INDEX IF NOT EXISTS attachment_intervention ON attachment
    FIELDS intervention, created_at;
DEFINE INDEX IF NOT EXISTS attachment_technical_note ON attachment
    FIELDS technical_note, created_at;
DEFINE INDEX IF NOT EXISTS attachment_storage_state ON attachment
    FIELDS storage_state, updated_at;

DEFINE EVENT IF NOT EXISTS attachment_validate_owner ON TABLE attachment
    WHEN $event IN ['CREATE', 'UPDATE']
    THEN {
        IF (
            $after.vehicle IS NONE
            AND $after.intervention IS NONE
            AND $after.technical_note IS NONE
        ) OR (
            $after.vehicle IS NOT NONE
            AND $after.intervention IS NOT NONE
        ) OR (
            $after.vehicle IS NOT NONE
            AND $after.technical_note IS NOT NONE
        ) OR (
            $after.intervention IS NOT NONE
            AND $after.technical_note IS NOT NONE
        ) {
            THROW 'attachment must have exactly one owner';
        };
    };

DEFINE EVENT IF NOT EXISTS attachment_validate_storage ON TABLE attachment
    WHEN $event IN ['CREATE', 'UPDATE']
    THEN {
        IF $after.storage_state = 'metadata_only' AND (
            $after.file IS NOT NONE OR $after.sha256 IS NOT NONE
        ) {
            THROW 'metadata-only attachment cannot claim stored bytes';
        };
        IF $after.storage_state = 'pending' AND (
            $after.file IS NONE
            OR $after.byte_size IS NOT NONE
            OR $after.sha256 IS NOT NONE
        ) {
            THROW 'pending attachment storage fields are invalid';
        };
        IF $after.storage_state = 'stored' AND (
            $after.file IS NONE
            OR $after.byte_size IS NONE
            OR $after.byte_size < 1
            OR $after.sha256 IS NONE
        ) {
            THROW 'stored attachment storage fields are required';
        };
        IF $after.storage_state = 'deleting' AND (
            $after.file IS NONE
            OR (($after.byte_size IS NONE) != ($after.sha256 IS NONE))
            OR ($after.byte_size IS NOT NONE AND $after.byte_size < 1)
        ) {
            THROW 'deleting attachment storage fields are invalid';
        };
    };
