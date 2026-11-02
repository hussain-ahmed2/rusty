//! Integration tests for the developer-facing `view!` + SSR path.

use rusty::prelude::*;

fn render(element: &Element) -> String {
    render_to_string(element).expect("rendering should succeed")
}

#[test]
fn acceptance_example_renders_exact_html() {
    let app = view! {
        <main>
            <h1>"Hello from Rusty"</h1>
            <p>"A Next.js-inspired framework built with Rust."</p>
        </main>
    };

    assert_eq!(
        render(&app),
        "<main><h1>Hello from Rusty</h1><p>A Next.js-inspired framework built with Rust.</p></main>"
    );
}

#[test]
fn renders_static_boolean_and_keyword_attributes() {
    let app = view! {
        <my-widget data-id="42" aria-label="Widget" hidden r#type="panel">
            "content"
        </my-widget>
    };

    assert_eq!(
        render(&app),
        "<my-widget data-id=\"42\" aria-label=\"Widget\" hidden type=\"panel\">content</my-widget>"
    );
}

#[test]
fn renders_fragments_from_multiple_roots() {
    let app = view! {
        <h1>"Title"</h1>
        <p>"Body"</p>
    };

    assert_eq!(render(&app), "<h1>Title</h1><p>Body</p>");
}

#[test]
fn renders_an_empty_view_as_an_empty_fragment() {
    let app = view! {};
    assert_eq!(render(&app), "");
}

#[test]
fn escapes_text_and_attribute_values() {
    let app = view! {
        <p title="\"quoted\" & <tag> 'single'">
            "5 < 6 & 7 > 3"
        </p>
    };

    assert_eq!(
        render(&app),
        "<p title=\"&quot;quoted&quot; &amp; &lt;tag&gt; &#x27;single&#x27;\">5 &lt; 6 &amp; 7 &gt; 3</p>"
    );
}

#[test]
fn supports_void_elements_and_self_closing_syntax() {
    let app = view! {
        <div>
            <br>
            <img src="/logo.png" alt="Rusty" />
            <span>"after"</span>
            <hr />
        </div>
    };

    assert_eq!(
        render(&app),
        "<div><br><img src=\"/logo.png\" alt=\"Rusty\"><span>after</span><hr></div>"
    );
}

#[test]
fn self_closing_non_void_elements_render_an_empty_pair() {
    let app = view! {
        <div>
            <section />
        </div>
    };

    assert_eq!(render(&app), "<div><section></section></div>");
}

#[test]
fn supports_namespaced_attributes() {
    let app = view! {
        <svg xmlns:xlink="http://www.w3.org/1999/xlink"></svg>
    };

    assert_eq!(
        render(&app),
        "<svg xmlns:xlink=\"http://www.w3.org/1999/xlink\"></svg>"
    );
}
