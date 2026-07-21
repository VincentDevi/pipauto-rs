DEFINE BUCKET IF NOT EXISTS pipauto_attachments
    BACKEND 'file:/home/nonroot/pipauto_attachments'
    PERMISSIONS NONE;
