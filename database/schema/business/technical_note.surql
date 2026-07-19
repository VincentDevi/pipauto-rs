DEFINE ANALYZER IF NOT EXISTS workshop_terms TOKENIZERS blank, class FILTERS lowercase;

DEFINE TABLE IF NOT EXISTS technical_note SCHEMAFULL;

DEFINE FIELD IF NOT EXISTS title ON technical_note TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 200;

DEFINE FIELD IF NOT EXISTS body ON technical_note TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 50000;

DEFINE FIELD IF NOT EXISTS tags ON technical_note TYPE array<string> DEFAULT []
    ASSERT array::len($value) <= 20
        AND array::distinct($value) = $value;

DEFINE FIELD IF NOT EXISTS tags.* ON technical_note TYPE string
    ASSERT string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 64
        AND $value = string::lowercase($value);

DEFINE FIELD IF NOT EXISTS vehicle ON technical_note TYPE option<record<vehicle>>
    REFERENCE ON DELETE REJECT
    ASSERT $value IS NONE OR record::exists($value);

DEFINE FIELD IF NOT EXISTS source_intervention ON technical_note TYPE option<record<intervention>>
    REFERENCE ON DELETE REJECT
    ASSERT $value IS NONE OR record::exists($value);

DEFINE FIELD IF NOT EXISTS make ON technical_note TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 80
    );

DEFINE FIELD IF NOT EXISTS make_normalized ON technical_note TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 240
        AND $value = string::lowercase($value)
    );

DEFINE FIELD IF NOT EXISTS model ON technical_note TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 80
    );

DEFINE FIELD IF NOT EXISTS model_normalized ON technical_note TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 240
        AND $value = string::lowercase($value)
    );

DEFINE FIELD IF NOT EXISTS engine ON technical_note TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 160
    );

DEFINE FIELD IF NOT EXISTS engine_normalized ON technical_note TYPE option<string>
    ASSERT $value IS NONE OR (
        string::trim($value) = $value
        AND string::len($value) >= 1
        AND string::len($value) <= 480
        AND $value = string::lowercase($value)
    );

DEFINE FIELD IF NOT EXISTS created_at ON technical_note TYPE datetime
    DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS updated_at ON technical_note TYPE datetime
    DEFAULT ALWAYS time::now();
DEFINE FIELD IF NOT EXISTS archived_at ON technical_note TYPE option<datetime>;

DEFINE INDEX IF NOT EXISTS technical_note_title_search ON technical_note
    FIELDS title FULLTEXT ANALYZER workshop_terms BM25;
DEFINE INDEX IF NOT EXISTS technical_note_body_search ON technical_note
    FIELDS body FULLTEXT ANALYZER workshop_terms BM25;
DEFINE INDEX IF NOT EXISTS technical_note_tags ON technical_note FIELDS tags;
DEFINE INDEX IF NOT EXISTS technical_note_make_normalized ON technical_note FIELDS make_normalized;
DEFINE INDEX IF NOT EXISTS technical_note_model_normalized ON technical_note FIELDS model_normalized;
DEFINE INDEX IF NOT EXISTS technical_note_engine_normalized ON technical_note
    FIELDS engine_normalized;
DEFINE INDEX IF NOT EXISTS technical_note_vehicle ON technical_note FIELDS vehicle;
DEFINE INDEX IF NOT EXISTS technical_note_source_intervention ON technical_note
    FIELDS source_intervention;
DEFINE INDEX IF NOT EXISTS technical_note_archived_at ON technical_note FIELDS archived_at;

DEFINE EVENT IF NOT EXISTS technical_note_validate_context ON TABLE technical_note
    WHEN $event IN ['CREATE', 'UPDATE']
    THEN {
        IF ($after.make IS NONE) != ($after.make_normalized IS NONE)
            OR ($after.model IS NONE) != ($after.model_normalized IS NONE)
            OR ($after.engine IS NONE) != ($after.engine_normalized IS NONE) {
            THROW 'technical-note display and normalized context must be paired';
        };
    };
