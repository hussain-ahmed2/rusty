//! Reactivity example for Rusty.

use rusty::prelude::*;

fn main() {
    let (owner, (count, page)) = create_root(|| {
        let count = Signal::new(1_u32);
        let doubled = Memo::new({
            let count = count.clone();
            move || count.get() * 2
        });

        let _effect = create_effect({
            let doubled = doubled.clone();
            move || println!("doubled: {}", doubled.get())
        });

        batch(|| {
            count.set(2);
            count.set(3);
        });

        let page = view! {
            <main>
                <h1>"Rusty reactivity"</h1>
                <p>{format!("count = {}", count.get())}</p>
                <p>{format!("doubled = {}", doubled.get())}</p>
            </main>
        };

        (count, page)
    });

    println!(
        "{}",
        render_to_string(&page).expect("rendering should succeed")
    );
    count.set(4);
    owner.dispose();
}
