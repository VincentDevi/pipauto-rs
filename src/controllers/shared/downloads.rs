//! Private attachment-content response construction shared by JSON and browser routes.

use axum::{
    body::Body,
    http::{
        header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE},
        HeaderValue, StatusCode,
    },
    response::Response,
};

use crate::{
    errors::AppError,
    models::attachment::{AttachmentContent, AttachmentMediaType},
};

pub(crate) fn content_response(
    content: AttachmentContent,
    force_download: bool,
) -> Result<Response, AppError> {
    let disposition = if force_download || !inline_capable(content.attachment.media_type) {
        "attachment"
    } else {
        "inline"
    };
    let disposition = content_disposition(disposition, &content.attachment.display_name);
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content.attachment.media_type.as_str())
        .header(CONTENT_LENGTH, content.attachment.byte_size.to_string())
        .header(CONTENT_DISPOSITION, disposition)
        .header(CACHE_CONTROL, "private, no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(content.bytes))
        .map_err(|_| AppError::Internal)
}

const fn inline_capable(media_type: AttachmentMediaType) -> bool {
    matches!(
        media_type,
        AttachmentMediaType::Pdf
            | AttachmentMediaType::Jpeg
            | AttachmentMediaType::Png
            | AttachmentMediaType::Webp
    )
}

fn content_disposition(kind: &str, display_name: &str) -> HeaderValue {
    let fallback: String = display_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, ' ' | '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .filter(|character| !character.is_control())
        .collect();
    let fallback = fallback.trim_matches([' ', '.']);
    let fallback = if fallback.is_empty() {
        "attachment"
    } else {
        fallback
    };
    let encoded = percent_encode_filename(display_name);
    HeaderValue::from_str(&format!(
        "{kind}; filename=\"{fallback}\"; filename*=UTF-8''{encoded}"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"attachment\""))
}

fn percent_encode_filename(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'!' | b'#' | b'$' | b'&' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
            )
        {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_security_content_disposition_resists_injection_for_every_media_type() {
        for media_type in [
            AttachmentMediaType::Pdf,
            AttachmentMediaType::Jpeg,
            AttachmentMediaType::Png,
            AttachmentMediaType::Webp,
        ] {
            assert!(inline_capable(media_type));
        }
        for media_type in [AttachmentMediaType::Heic, AttachmentMediaType::Heif] {
            assert!(!inline_capable(media_type));
        }

        let value = content_disposition("inline", "report\r\nété.pdf");
        let value = value.to_str().expect("safe disposition header");
        assert!(!value.contains('\r'));
        assert!(!value.contains('\n'));
        assert!(value.contains("filename=\"report___t_.pdf\""));
        assert!(value.contains("filename*=UTF-8''report%0D%0A%C3%A9t%C3%A9.pdf"));
    }
}
