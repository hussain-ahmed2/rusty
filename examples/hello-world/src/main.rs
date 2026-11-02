//! Example application demonstrating Rusty's component system.

use rusty::prelude::*;

#[component]
fn Layout(children: Children) -> Element {
    view! { <main class="app">{children}</main> }
}

#[component]
fn Greeting(name: String) -> Element {
    view! { <h1>{format!("Hello from {name}")}</h1> }
}

#[component]
fn Home() -> Element {
    let features = [
        "Components",
        "Typed props",
        "Children",
        "Conditionals",
        "Iteration",
    ];

    view! {
        <Layout>
            <Greeting name="Rusty" />
            <p>"A Next.js-inspired framework built with Rust."</p>
            <ul>
                {#each features.iter() as feature}
                    <li>{*feature}</li>
                {/each}
            </ul>
        </Layout>
    }
}

fn main() {
    let app = view! { <Home /> };
    println!("{}", render_to_string(&app).unwrap());
}
