pub(crate) fn document(html: &str, base_url: Option<&str>) -> String {
    match base_url {
        Some(base) => format!("<base href=\"{}\">{html}", attribute(base)),
        None => html.to_owned(),
    }
}

fn attribute(value: &str) -> String {
    value.replace('&', "&amp;").replace('"', "&quot;")
}
