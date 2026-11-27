#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebContent {
    Url(String),
    Html {
        html: String,
        base_url: Option<String>,
    },
}

impl WebContent {
    pub fn url(url: impl Into<String>) -> Self {
        WebContent::Url(url.into())
    }

    pub fn html(html: impl Into<String>) -> Self {
        WebContent::Html {
            html: html.into(),
            base_url: None,
        }
    }

    pub fn base_url(self, base: impl Into<String>) -> Self {
        match self {
            WebContent::Html { html, .. } => WebContent::Html {
                html,
                base_url: Some(base.into()),
            },
            url @ WebContent::Url(_) => url,
        }
    }
}
