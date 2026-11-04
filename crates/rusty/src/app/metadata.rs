//! Document metadata produced by `metadata.rs` route modules.

/// A metadata attribute kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MetaKind {
    Name,
    Property,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MetaTag {
    kind: MetaKind,
    key: String,
    content: String,
}

/// Document metadata for a page.
///
/// Metadata modules export `pub fn metadata(context: &mut RequestContext) -> Metadata`.
/// Root metadata is inherited by child routes, and the nearest metadata module
/// overrides scalar values such as the title and description.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    title: Option<String>,
    description: Option<String>,
    canonical: Option<String>,
    tags: Vec<MetaTag>,
}

impl Metadata {
    /// Creates empty metadata.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the document title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the meta description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the canonical URL.
    #[must_use]
    pub fn with_canonical(mut self, canonical: impl Into<String>) -> Self {
        self.canonical = Some(canonical.into());
        self
    }

    /// Adds a `name` meta tag.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>, content: impl Into<String>) -> Self {
        self.set_tag(MetaKind::Name, name.into(), content.into());
        self
    }

    /// Adds a `property` meta tag.
    #[must_use]
    pub fn with_property(
        mut self,
        property: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        self.set_tag(MetaKind::Property, property.into(), content.into());
        self
    }

    /// Returns the document title.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Returns the meta description.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the canonical URL.
    #[must_use]
    pub fn canonical(&self) -> Option<&str> {
        self.canonical.as_deref()
    }

    /// Returns `true` when no metadata is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.canonical.is_none()
            && self.tags.is_empty()
    }

    fn set_tag(&mut self, kind: MetaKind, key: String, content: String) {
        if let Some(tag) = self
            .tags
            .iter_mut()
            .find(|tag| tag.kind == kind && tag.key.eq_ignore_ascii_case(&key))
        {
            tag.content = content;
        } else {
            self.tags.push(MetaTag { kind, key, content });
        }
    }

    pub(crate) fn merge(mut self, other: Self) -> Self {
        if other.title.is_some() {
            self.title = other.title;
        }
        if other.description.is_some() {
            self.description = other.description;
        }
        if other.canonical.is_some() {
            self.canonical = other.canonical;
        }
        for tag in other.tags {
            self.set_tag(tag.kind, tag.key, tag.content);
        }
        self
    }

    pub(crate) fn has_title(&self) -> bool {
        self.title.is_some()
    }

    pub(crate) fn should_replace_meta(&self, name: Option<&str>, property: Option<&str>) -> bool {
        if self.description.is_some() && matches_ignore_ascii_case(name, "description") {
            return true;
        }
        self.tags.iter().any(|tag| match tag.kind {
            MetaKind::Name => matches_ignore_ascii_case(name, &tag.key),
            MetaKind::Property => matches_ignore_ascii_case(property, &tag.key),
        })
    }

    pub(crate) fn markup(&self) -> String {
        let mut markup = String::new();
        if let Some(title) = &self.title {
            markup.push_str("<title>");
            escape_text(title, &mut markup);
            markup.push_str("</title>");
        }
        if let Some(description) = &self.description {
            markup.push_str("<meta name=\"description\" content=\"");
            escape_attribute(description, &mut markup);
            markup.push_str("\">");
        }
        if let Some(canonical) = &self.canonical {
            markup.push_str("<link rel=\"canonical\" href=\"");
            escape_attribute(canonical, &mut markup);
            markup.push_str("\">");
        }
        for tag in &self.tags {
            let attribute = match tag.kind {
                MetaKind::Name => "name",
                MetaKind::Property => "property",
            };
            markup.push_str("<meta ");
            markup.push_str(attribute);
            markup.push_str("=\"");
            escape_attribute(&tag.key, &mut markup);
            markup.push_str("\" content=\"");
            escape_attribute(&tag.content, &mut markup);
            markup.push_str("\">");
        }
        markup
    }
}

fn matches_ignore_ascii_case(value: Option<&str>, expected: &str) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn escape_text(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(character),
        }
    }
}

fn escape_attribute(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(character),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_lets_nearer_values_override() {
        let metadata = Metadata::new()
            .with_title("Root")
            .with_description("Root description")
            .with_property("og:title", "Root")
            .merge(
                Metadata::new()
                    .with_title("Page")
                    .with_property("og:title", "Page")
                    .with_name("twitter:card", "summary"),
            );

        assert_eq!(metadata.title(), Some("Page"));
        assert_eq!(metadata.description(), Some("Root description"));
        assert_eq!(
            metadata.markup(),
            "<title>Page</title><meta name=\"description\" content=\"Root description\"><meta property=\"og:title\" content=\"Page\"><meta name=\"twitter:card\" content=\"summary\">"
        );
    }

    #[test]
    fn markup_escapes_values() {
        let metadata = Metadata::new()
            .with_title("Fish & <chips>")
            .with_description("\"quoted\"")
            .with_canonical("https://example.test/?a=1&b=2");

        assert_eq!(
            metadata.markup(),
            "<title>Fish &amp; &lt;chips&gt;</title><meta name=\"description\" content=\"&quot;quoted&quot;\"><link rel=\"canonical\" href=\"https://example.test/?a=1&amp;b=2\">"
        );
    }
}
