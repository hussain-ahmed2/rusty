//! Integration tests for components, typed props, children, dynamic
//! expressions, conditionals, and iteration.

use rusty::prelude::*;

#[component]
fn Layout(children: Children) -> Element {
    view! { <main class="layout">{children}</main> }
}

#[component]
fn Greeting(name: String) -> Element {
    view! { <p class="greeting">{format!("Hello, {name}!")}</p> }
}

#[component]
fn Card(title: String, children: Children) -> Element {
    view! {
        <section class="card">
            <h2>{title}</h2>
            {children}
        </section>
    }
}

#[component]
fn ElementChildren(children: Element) -> Element {
    view! { <div>{children}</div> }
}

#[component]
fn Typed(r#type: String) -> Element {
    view! { <span class={r#type}>"typed"</span> }
}

#[component]
#[inline]
fn Inlined() -> Element {
    view! { <p>"inlined"</p> }
}

fn render(element: &Element) -> String {
    render_to_string(element).expect("rendering should succeed")
}

#[test]
fn renders_component_with_static_string_prop() {
    let app = view! { <Greeting name="Rusty" /> };
    assert_eq!(render(&app), "<p class=\"greeting\">Hello, Rusty!</p>");
}

#[test]
fn renders_component_with_dynamic_prop() {
    let name = String::from("World");
    let app = view! { <Greeting name={name.clone()} /> };
    assert_eq!(render(&app), "<p class=\"greeting\">Hello, World!</p>");
}

#[test]
fn renders_children_in_a_layout() {
    let app = view! {
        <Layout>
            <h1>"Title"</h1>
            <p>"Body"</p>
        </Layout>
    };

    assert_eq!(
        render(&app),
        "<main class=\"layout\"><h1>Title</h1><p>Body</p></main>"
    );
}

#[test]
fn paired_component_with_no_children_renders_empty_children() {
    let app = view! { <Layout></Layout> };
    assert_eq!(render(&app), "<main class=\"layout\"></main>");
}

#[test]
fn self_closing_component_can_pass_children_prop() {
    let content = Element::text("from prop");
    let app = view! { <Layout children={content} /> };
    assert_eq!(render(&app), "<main class=\"layout\">from prop</main>");
}

#[test]
fn function_attributes_are_preserved() {
    let app = view! { <Inlined /> };
    assert_eq!(render(&app), "<p>inlined</p>");
}

#[test]
fn composes_components_with_props_and_children() {
    let app = view! {
        <Card title="Welcome">
            <Greeting name="Rusty" />
        </Card>
    };

    assert_eq!(
        render(&app),
        "<section class=\"card\"><h2>Welcome</h2><p class=\"greeting\">Hello, Rusty!</p></section>"
    );
}

#[test]
fn children_can_be_typed_as_element() {
    let app = view! {
        <ElementChildren>
            <p>"First"</p>
            <p>"Second"</p>
        </ElementChildren>
    };

    assert_eq!(render(&app), "<div><p>First</p><p>Second</p></div>");
}

#[test]
fn supports_raw_identifier_props() {
    let app = view! { <Typed r#type="label" /> };
    assert_eq!(render(&app), "<span class=\"label\">typed</span>");
}

#[test]
fn renders_dynamic_text_and_attributes() {
    let count = 3_u32;
    let name = String::from("Rusty");
    let class = Some("active");

    let app = view! {
        <div data-count={count} class={class}>
            <p>{name.clone()}</p>
        </div>
    };

    assert_eq!(
        render(&app),
        "<div data-count=\"3\" class=\"active\"><p>Rusty</p></div>"
    );
}

#[test]
fn omitted_dynamic_attributes_are_not_rendered() {
    let missing: Option<&str> = None;
    let disabled = false;

    let app = view! {
        <input class={missing} disabled={disabled}>
    };

    assert_eq!(render(&app), "<input>");
}

#[test]
fn renders_if_else_branches() {
    fn page(logged_in: bool) -> Element {
        view! {
            <div>
                {#if logged_in}
                    <span>"dashboard"</span>
                {:else}
                    <span>"login"</span>
                {/if}
            </div>
        }
    }

    assert_eq!(render(&page(true)), "<div><span>dashboard</span></div>");
    assert_eq!(render(&page(false)), "<div><span>login</span></div>");
}

#[test]
fn renders_else_if_chains() {
    fn label(value: u8) -> Element {
        view! {
            {#if value == 1}
                "one"
            {:else if value == 2}
                "two"
            {:else}
                "many"
            {/if}
        }
    }

    assert_eq!(render(&label(1)), "one");
    assert_eq!(render(&label(2)), "two");
    assert_eq!(render(&label(9)), "many");
}

#[test]
fn renders_iteration() {
    let names = ["Ada", "Linus", "Margaret"];

    let app = view! {
        <ul>
            {#each names.iter() as name}
                <li>{*name}</li>
            {/each}
        </ul>
    };

    assert_eq!(
        render(&app),
        "<ul><li>Ada</li><li>Linus</li><li>Margaret</li></ul>"
    );
}

#[test]
fn renders_iteration_with_components_and_destructuring() {
    let users = [(String::from("Ada"), 36_u8), (String::from("Linus"), 54_u8)];

    let app = view! {
        <div>
            {#each users.iter() as (name, age)}
                <p>{format!("{name}: {age}")}</p>
            {/each}
        </div>
    };

    assert_eq!(render(&app), "<div><p>Ada: 36</p><p>Linus: 54</p></div>");
}

#[test]
fn renders_nested_control_flow() {
    let values = [1_u8, 2, 3];

    let app = view! {
        <div>
            {#each values.iter() as value}
                {#if *value % 2 == 1}
                    <span>{*value}</span>
                {:else}
                    <strong>{*value}</strong>
                {/if}
            {/each}
        </div>
    };

    assert_eq!(
        render(&app),
        "<div><span>1</span><strong>2</strong><span>3</span></div>"
    );
}

mod ui {
    use super::*;

    /// A component defined outside the call site and imported by name.
    #[component]
    pub fn Badge(label: String) -> Element {
        view! { <span class="badge">{label}</span> }
    }
}

#[test]
fn renders_imported_components_with_public_props() {
    use ui::Badge;

    let app = view! { <Badge label="new" /> };
    assert_eq!(render(&app), "<span class=\"badge\">new</span>");
}
