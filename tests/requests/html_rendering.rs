use std::{fs, path::Path};

#[test]
fn html_rendering_fragments_are_single_document_safe_replacement_units() {
    for path in html_files("assets/views/fragments") {
        let html = fs::read_to_string(&path).expect("fragment should be readable");
        let lowercase = html.to_ascii_lowercase();
        for forbidden in [
            "<!doctype",
            "<html>",
            "<html ",
            "<head>",
            "<head ",
            "<body>",
            "<body ",
            "<main>",
            "<main ",
        ] {
            assert!(
                !lowercase.contains(forbidden),
                "{} can nest a full document through {forbidden}",
                path.display()
            );
        }
    }
}

#[test]
fn html_rendering_pages_use_the_shared_landmarked_shell() {
    let layout = fs::read_to_string("assets/views/layouts/base.html").expect("base layout");
    assert!(layout.contains("class=\"skip-link\""));
    assert!(layout.contains("<main id=\"main-content\""));
    assert!(layout.contains("aria-label=\"Primary navigation\""));
    assert!(layout.contains("aria-live=\"polite\""));

    for path in html_files("assets/views/pages") {
        let html = fs::read_to_string(&path).expect("page should be readable");
        assert!(
            html.contains("{% extends \"layouts/base.html\" %}"),
            "{} bypasses the shared document shell",
            path.display()
        );
    }
}

#[test]
fn html_rendering_keeps_user_authored_values_escaped() {
    for path in html_files("assets/views") {
        let html = fs::read_to_string(&path).expect("template should be readable");
        if !html.contains("| safe") {
            continue;
        }
        assert_eq!(
            path,
            Path::new("assets/views/fragments/intervention_preview.html"),
            "{} disables escaping outside the reviewed server-generated dashboard URLs",
            path.display()
        );
        for safe_expression in html.match_indices("| safe") {
            let prefix = &html[..safe_expression.0];
            let expression = prefix.rsplit("{{").next().unwrap_or_default();
            assert!(
                [
                    "section.collection_href ",
                    "section.retry_path ",
                    "item.href "
                ]
                .iter()
                .any(|allowed| expression.ends_with(allowed)),
                "unreviewed safe expression in {}",
                path.display()
            );
        }
    }
}

fn html_files(directory: &str) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(Path::new(directory)).expect("template directory") {
        let path = entry.expect("template entry").path();
        if path.is_dir() {
            files.extend(html_files(path.to_str().expect("UTF-8 path")));
        } else if path
            .extension()
            .is_some_and(|extension| extension == "html")
        {
            files.push(path);
        }
    }
    files
}
