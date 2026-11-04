//! Dynamic documentation page.

use rusty::prelude::*;

/// Renders a documentation page selected by its slug.
pub fn page(context: &mut RequestContext) -> Result<Element, ServerError> {
    let docs = crate::content::url("docs/");
    let Some(doc) = context
        .param("slug")
        .and_then(crate::content::doc)
    else {
        return Ok(view! {
            <header class="page-heading">
                <p class="eyebrow">"Documentation"</p>
                <h1>"Page not found"</h1>
                <p class="lead">"This documentation route does not exist."</p>
                <p><a class="text-link" href={docs}>"Back to the documentation index"</a></p>
            </header>
        });
    };

    let title = doc.title;
    let summary = doc.summary;
    let sections = Element::fragment(
        doc.sections
            .iter()
            .map(|section| {
                let title = section.title;
                let body = crate::content::rich(section.body);
                view! { <section><h2>{title}</h2><p>{body}</p></section> }
            })
            .collect::<Vec<_>>(),
    );

    Ok(view! {
        <header class="page-heading">
            <p class="eyebrow">"Documentation"</p>
            <h1>{title}</h1>
            <p class="lead">{summary}</p>
        </header>
        {sections}
        <hr>
        <p><a class="text-link" href={docs}>"All documentation"</a></p>
    })
}
