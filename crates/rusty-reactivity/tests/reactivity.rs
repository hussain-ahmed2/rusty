//! Integration tests for signals, memos, effects, batching, ownership, and context.

use std::cell::{Cell, RefCell};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use rusty_core::{AttributeValue, Element, IntoAttributeValue, IntoElement};
use rusty_reactivity::{
    batch, create_effect, create_root, expect_context, memo, on_cleanup, provide_context, signal,
    untrack, use_context, Effect, Memo, Signal,
};

#[test]
fn signal_reads_and_writes() {
    let value = signal(1_u32);

    assert_eq!(value.get(), 1);
    value.set(2);
    value.update(|current| *current += 1);
    assert_eq!(value.get(), 3);
    assert_eq!(value.with(|current| *current), 3);

    assert!(!value.set_if_changed(3));
    assert!(value.set_if_changed(4));
    assert_eq!(value.get_untracked(), 4);
}

#[test]
fn effect_runs_immediately_and_reruns_on_change() {
    let (owner, (value, runs)) = create_root(|| {
        let value = Signal::new(0_u32);
        let runs = Rc::new(Cell::new(0));

        let _effect = create_effect({
            let value = value.clone();
            let runs = Rc::clone(&runs);
            move || {
                let _ = value.get();
                runs.set(runs.get() + 1);
            }
        });

        (value, runs)
    });

    assert_eq!(runs.get(), 1);
    value.set(1);
    assert_eq!(runs.get(), 2);

    owner.dispose();
}

#[test]
fn batch_coalesces_notifications_and_nests() {
    let (owner, (value, runs)) = create_root(|| {
        let value = Signal::new(0_u32);
        let runs = Rc::new(Cell::new(0));

        let _effect = create_effect({
            let value = value.clone();
            let runs = Rc::clone(&runs);
            move || {
                let _ = value.get();
                runs.set(runs.get() + 1);
            }
        });

        (value, runs)
    });

    batch(|| {
        value.set(1);
        value.set(2);
        value.set(3);
    });
    assert_eq!(runs.get(), 2);

    batch(|| {
        value.set(4);
        batch(|| value.set(5));
        value.set(6);
    });
    assert_eq!(runs.get(), 3);

    owner.dispose();
}

#[test]
fn untracked_reads_do_not_subscribe() {
    let (owner, (value, runs, untracked_runs)) = create_root(|| {
        let value = Signal::new(0_u32);
        let runs = Rc::new(Cell::new(0));
        let untracked_runs = Rc::new(Cell::new(0));

        let _effect = create_effect({
            let value = value.clone();
            let runs = Rc::clone(&runs);
            move || {
                let _ = value.get();
                runs.set(runs.get() + 1);
            }
        });

        let _untracked_effect = create_effect({
            let value = value.clone();
            let untracked_runs = Rc::clone(&untracked_runs);
            move || {
                untrack(|| value.get());
                untracked_runs.set(untracked_runs.get() + 1);
            }
        });

        (value, runs, untracked_runs)
    });

    value.set(1);

    assert_eq!(runs.get(), 2);
    assert_eq!(untracked_runs.get(), 1);

    owner.dispose();
}

#[test]
fn writes_inside_untrack_still_flush_with_tracking() {
    let (owner, (value, runs)) = create_root(|| {
        let value = Signal::new(0_u32);
        let runs = Rc::new(Cell::new(0));

        let _effect = create_effect({
            let value = value.clone();
            let runs = Rc::clone(&runs);
            move || {
                let _ = value.get();
                runs.set(runs.get() + 1);
            }
        });

        (value, runs)
    });

    assert_eq!(runs.get(), 1);

    untrack(|| value.set(1));
    assert_eq!(runs.get(), 2);

    value.set(2);
    assert_eq!(runs.get(), 3);

    owner.dispose();
}

#[test]
fn untracked_memo_recomputation_keeps_its_dependencies() {
    let (owner, (value, doubled, computes)) = create_root(|| {
        let value = Signal::new(1_u32);
        let computes = Rc::new(Cell::new(0));
        let doubled = Memo::new({
            let value = value.clone();
            let computes = Rc::clone(&computes);
            move || {
                computes.set(computes.get() + 1);
                value.get() * 2
            }
        });

        (value, doubled, computes)
    });

    assert_eq!(computes.get(), 0);
    assert_eq!(untrack(|| doubled.get()), 2);
    assert_eq!(computes.get(), 1);

    value.set(2);
    assert_eq!(doubled.get(), 4);
    assert_eq!(computes.get(), 2);

    owner.dispose();
}

#[test]
fn effect_cleanups_run_before_reruns_and_on_dispose() {
    let (owner, (value, cleanups, effect)) = create_root(|| {
        let value = Signal::new(0_u32);
        let cleanups = Rc::new(Cell::new(0));

        let effect = create_effect({
            let value = value.clone();
            let cleanups = Rc::clone(&cleanups);
            move || {
                let _ = value.get();
                let cleanups = Rc::clone(&cleanups);
                on_cleanup(move || cleanups.set(cleanups.get() + 1));
            }
        });

        (value, cleanups, effect)
    });

    assert_eq!(cleanups.get(), 0);
    value.set(1);
    assert_eq!(cleanups.get(), 1);

    effect.dispose();
    assert_eq!(cleanups.get(), 2);
    assert!(effect.is_disposed());

    value.set(2);
    assert_eq!(cleanups.get(), 2);

    owner.dispose();
}

#[test]
fn disposing_an_owner_disposes_effects() {
    let (owner, (value, runs, cleanups)) = create_root(|| {
        let value = Signal::new(0_u32);
        let runs = Rc::new(Cell::new(0));
        let cleanups = Rc::new(Cell::new(0));

        let _effect = create_effect({
            let value = value.clone();
            let runs = Rc::clone(&runs);
            let cleanups = Rc::clone(&cleanups);
            move || {
                let _ = value.get();
                runs.set(runs.get() + 1);
                let cleanups = Rc::clone(&cleanups);
                on_cleanup(move || cleanups.set(cleanups.get() + 1));
            }
        });

        (value, runs, cleanups)
    });

    assert_eq!(runs.get(), 1);
    owner.dispose();
    assert_eq!(cleanups.get(), 1);

    value.set(1);
    assert_eq!(runs.get(), 1);
}

#[test]
fn parent_disposal_disposes_child_owners() {
    let (outer, (child, cleanups)) = create_root(|| {
        let (child, cleanups) = create_root(|| {
            let cleanups = Rc::new(Cell::new(0));

            let _effect = create_effect({
                let cleanups = Rc::clone(&cleanups);
                move || {
                    let cleanups = Rc::clone(&cleanups);
                    on_cleanup(move || cleanups.set(cleanups.get() + 1));
                }
            });

            cleanups
        });

        (child, cleanups)
    });

    assert!(!child.is_disposed());
    outer.dispose();
    assert!(child.is_disposed());
    assert_eq!(cleanups.get(), 1);
    drop(child);
}

#[test]
fn context_uses_the_nearest_owner() {
    let (outer, ()) = create_root(|| {
        provide_context(String::from("outer"));
        assert_eq!(*expect_context::<String>(), "outer");
        assert!(use_context::<u32>().is_none());

        let (child, ()) = create_root(|| {
            assert_eq!(*expect_context::<String>(), "outer");
            provide_context(String::from("child"));
            assert_eq!(*expect_context::<String>(), "child");

            let (grandchild, ()) = create_root(|| {
                assert_eq!(*expect_context::<String>(), "child");
            });
            grandchild.dispose();
        });

        child.dispose();
        assert_eq!(*expect_context::<String>(), "outer");
    });

    outer.dispose();
}

#[test]
fn owner_context_is_available_during_disposal() {
    let (owner, seen) = create_root(|| {
        provide_context(String::from("root"));
        let seen = Rc::new(RefCell::new(None));

        let cleanup_seen = Rc::clone(&seen);
        on_cleanup(move || {
            *cleanup_seen.borrow_mut() = Some(expect_context::<String>().to_string());
        });

        seen
    });

    owner.dispose();

    assert_eq!(seen.borrow().as_deref(), Some("root"));
}

#[test]
fn effects_run_in_scheduling_order() {
    let (owner, (value, order)) = create_root(|| {
        let value = Signal::new(0_u32);
        let order = Rc::new(RefCell::new(Vec::new()));

        for label in ["first", "second"] {
            let order = Rc::clone(&order);
            let value = value.clone();
            let _effect = create_effect(move || {
                let _ = value.get();
                order.borrow_mut().push(label);
            });
        }

        (value, order)
    });

    order.borrow_mut().clear();
    value.set(1);

    assert_eq!(*order.borrow(), ["first", "second"]);

    owner.dispose();
}

#[test]
fn memos_are_lazy_and_cached() {
    let (owner, (value, doubled, computes)) = create_root(|| {
        let value = Signal::new(2_u32);
        let computes = Rc::new(Cell::new(0));

        let doubled = Memo::new({
            let value = value.clone();
            let computes = Rc::clone(&computes);
            move || {
                computes.set(computes.get() + 1);
                value.get() * 2
            }
        });

        (value, doubled, computes)
    });

    assert_eq!(computes.get(), 0);
    assert_eq!(doubled.get(), 4);
    assert_eq!(computes.get(), 1);
    assert_eq!(doubled.get(), 4);
    assert_eq!(computes.get(), 1);

    value.set(3);
    assert_eq!(computes.get(), 1);
    assert_eq!(doubled.get(), 6);
    assert_eq!(computes.get(), 2);

    owner.dispose();
}

#[test]
fn memos_recompute_once_per_batch_for_subscribers() {
    let (owner, (value, doubled, runs, computes)) = create_root(|| {
        let value = Signal::new(1_u32);
        let runs = Rc::new(Cell::new(0));
        let computes = Rc::new(Cell::new(0));
        let doubled = Memo::new({
            let value = value.clone();
            let computes = Rc::clone(&computes);
            move || {
                computes.set(computes.get() + 1);
                value.get() * 2
            }
        });

        let _effect = create_effect({
            let doubled = doubled.clone();
            let runs = Rc::clone(&runs);
            move || {
                let _ = doubled.get();
                runs.set(runs.get() + 1);
            }
        });

        (value, doubled, runs, computes)
    });

    assert_eq!(runs.get(), 1);
    assert_eq!(computes.get(), 1);

    batch(|| {
        value.set(2);
        value.set(3);
    });

    assert_eq!(runs.get(), 2);
    assert_eq!(computes.get(), 2);
    assert_eq!(doubled.get(), 6);

    owner.dispose();
}

#[test]
fn memo_with_defers_invalidation_until_after_the_borrow() {
    let (owner, (value, doubled, runs)) = create_root(|| {
        let value = Signal::new(1_u32);
        let doubled = Memo::new({
            let value = value.clone();
            move || value.get() * 2
        });
        let runs = Rc::new(Cell::new(0));

        let _effect = create_effect({
            let doubled = doubled.clone();
            let runs = Rc::clone(&runs);
            move || {
                let _ = doubled.get();
                runs.set(runs.get() + 1);
            }
        });

        (value, doubled, runs)
    });

    assert_eq!(runs.get(), 1);

    doubled.with(|_| value.set(5));

    assert_eq!(runs.get(), 2);
    assert_eq!(doubled.get(), 10);

    owner.dispose();
}

#[test]
fn memo_untracked_reads_do_not_subscribe() {
    let (owner, (value, doubled, runs)) = create_root(|| {
        let value = Signal::new(1_u32);
        let runs = Rc::new(Cell::new(0));
        let doubled = Memo::new({
            let value = value.clone();
            move || value.get() * 2
        });

        let _effect = create_effect({
            let doubled = doubled.clone();
            let runs = Rc::clone(&runs);
            move || {
                let _ = doubled.get_untracked();
                runs.set(runs.get() + 1);
            }
        });

        (value, doubled, runs)
    });

    value.set(2);

    assert_eq!(runs.get(), 1);
    assert_eq!(doubled.get_untracked(), 4);

    owner.dispose();
}

#[test]
fn memo_cycles_are_reported() {
    let slot: Rc<RefCell<Option<Memo<u32>>>> = Rc::new(RefCell::new(None));
    let value = Memo::new({
        let slot = Rc::clone(&slot);
        move || {
            let memo = slot
                .borrow()
                .as_ref()
                .expect("memo is installed before reading")
                .clone();
            memo.get()
        }
    });
    *slot.borrow_mut() = Some(value.clone());

    let result = catch_unwind(AssertUnwindSafe(|| value.get()));

    assert!(result.is_err());
    slot.borrow_mut().take();
    drop(value);
}

#[test]
fn effects_can_trigger_other_effects() {
    let (owner, (source, derived, seen)) = create_root(|| {
        let source = Signal::new(0_u32);
        let derived = Signal::new(0_u32);
        let seen = Rc::new(Cell::new(0_u32));

        let _first = create_effect({
            let source = source.clone();
            let derived = derived.clone();
            move || derived.set(source.get() * 2)
        });

        let _second = create_effect({
            let derived = derived.clone();
            let seen = Rc::clone(&seen);
            move || seen.set(derived.get())
        });

        (source, derived, seen)
    });

    assert_eq!(seen.get(), 0);
    source.set(3);
    assert_eq!(derived.get(), 6);
    assert_eq!(seen.get(), 6);

    owner.dispose();
}

#[test]
fn signals_render_as_elements_and_attributes() {
    let text = Signal::new(String::from("hello"));
    assert_eq!(text.clone().into_element(), Element::text("hello"));
    assert_eq!((&text).into_element(), Element::text("hello"));
    assert_eq!(text.into_attribute_value(), AttributeValue::text("hello"));

    let flag = Signal::new(true);
    assert_eq!(flag.into_attribute_value(), AttributeValue::Bool(true));

    let number = memo(|| 42_u32);
    assert_eq!(number.clone().into_element(), Element::text("42"));
    assert_eq!((&number).into_attribute_value(), AttributeValue::text("42"));
}

#[test]
fn effect_handles_report_disposal() {
    let (owner, effect) = create_root(|| {
        let effect = Effect::new(|| {});
        assert!(!effect.is_disposed());
        effect
    });

    effect.dispose();
    assert!(effect.is_disposed());
    owner.dispose();
}
