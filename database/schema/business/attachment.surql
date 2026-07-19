DEFINE TABLE IF NOT EXISTS attachment SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS vehicle ON attachment TYPE option<record<vehicle>>
    REFERENCE ON DELETE REJECT
    ASSERT $value IS NONE OR record::exists($value);

DEFINE FIELD IF NOT EXISTS intervention ON attachment TYPE option<record<intervention>>
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
    ASSERT $value IS NONE OR $value >= 0;

DEFINE FIELD IF NOT EXISTS caption ON attachment TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 1000
    );

DEFINE FIELD IF NOT EXISTS storage_state ON attachment TYPE string DEFAULT 'metadata_only'
    ASSERT $value = 'metadata_only';

DEFINE FIELD IF NOT EXISTS created_at ON attachment TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS updated_at ON attachment TYPE datetime DEFAULT ALWAYS time::now();

DEFINE INDEX IF NOT EXISTS attachment_vehicle ON attachment FIELDS vehicle;
DEFINE INDEX IF NOT EXISTS attachment_intervention ON attachment FIELDS intervention;
DEFINE INDEX IF NOT EXISTS attachment_storage_state ON attachment FIELDS storage_state;

DEFINE EVENT IF NOT EXISTS attachment_validate_owner ON TABLE attachment
    WHEN $event IN ['CREATE', 'UPDATE']
    THEN {
        IF ($after.vehicle IS NONE) = ($after.intervention IS NONE) {
            THROW 'attachment metadata must have exactly one owner';
        };
    };
