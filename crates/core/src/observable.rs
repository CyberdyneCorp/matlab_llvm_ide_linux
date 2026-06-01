//! Reactive primitives that stand in for SwiftUI's `@Published`.
//!
//! The macOS reference uses `@Published` properties on `@MainActor`
//! `ObservableObject` view models; SwiftUI views observe them and re-render.
//! On GTK there is no built-in observation, so this module provides a tiny,
//! single-threaded, push-based [`Property<T>`]: it holds a value and a list of
//! subscriber closures, and notifies them whenever the value changes.
//!
//! Design constraints that keep this safe and testable:
//! * Single-threaded — the whole UI runs on the GTK main loop, mirroring the
//!   reference's `@MainActor` isolation. No locking, just `Rc`/`RefCell`.
//! * Re-entrancy safe — subscribers are stored behind `Rc` and a *snapshot* of
//!   the subscriber list is taken before notification, so a subscriber may
//!   freely call [`Property::set`] again (or subscribe/unsubscribe) without
//!   tripping a `RefCell` borrow panic.
//! * No GTK dependency — view models hold `Property`s and are unit-tested by
//!   subscribing and asserting the emitted values.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

/// Opaque handle returned by [`Property::subscribe`]; pass it to
/// [`Property::unsubscribe`] to stop receiving notifications.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct SubscriptionId(u64);

type Callback<T> = Rc<dyn Fn(&T)>;

struct Inner<T> {
    value: T,
    subscribers: Vec<(SubscriptionId, Callback<T>)>,
    next_id: u64,
}

/// A single observable value. Clone is cheap (`Rc` bump) and shares the same
/// underlying state, so a view model can hand a clone to a view and both see
/// the same updates.
pub struct Property<T> {
    inner: Rc<RefCell<Inner<T>>>,
}

impl<T> Clone for Property<T> {
    fn clone(&self) -> Self {
        Property { inner: Rc::clone(&self.inner) }
    }
}

impl<T: Clone + 'static> Property<T> {
    /// Create a property holding `value`.
    pub fn new(value: T) -> Self {
        Property {
            inner: Rc::new(RefCell::new(Inner {
                value,
                subscribers: Vec::new(),
                next_id: 0,
            })),
        }
    }

    /// Current value (cloned).
    pub fn get(&self) -> T {
        self.inner.borrow().value.clone()
    }

    /// Read the value by reference without cloning, via a closure.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.borrow().value)
    }

    /// Replace the value and notify every subscriber with the new value.
    ///
    /// Notification uses a snapshot of the subscriber list, so subscribers may
    /// re-enter (`set`, `subscribe`, `unsubscribe`) without deadlocking.
    pub fn set(&self, value: T) {
        let subscribers = {
            let mut inner = self.inner.borrow_mut();
            inner.value = value.clone();
            inner.subscribers.iter().map(|(_, cb)| Rc::clone(cb)).collect::<Vec<_>>()
        };
        for cb in subscribers {
            cb(&value);
        }
    }

    /// Mutate the value in place, then notify subscribers with the result.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let new_value = {
            let mut inner = self.inner.borrow_mut();
            f(&mut inner.value);
            inner.value.clone()
        };
        let subscribers = {
            let inner = self.inner.borrow();
            inner.subscribers.iter().map(|(_, cb)| Rc::clone(cb)).collect::<Vec<_>>()
        };
        for cb in subscribers {
            cb(&new_value);
        }
    }

    /// Set only if the new value differs (requires `PartialEq`). Returns whether
    /// a change (and notification) happened. Avoids redundant UI churn.
    pub fn set_if_changed(&self, value: T) -> bool
    where
        T: PartialEq,
    {
        if self.inner.borrow().value == value {
            return false;
        }
        self.set(value);
        true
    }

    /// Register `callback`, invoked on every subsequent [`set`](Property::set).
    pub fn subscribe(&self, callback: impl Fn(&T) + 'static) -> SubscriptionId {
        let mut inner = self.inner.borrow_mut();
        let id = SubscriptionId(inner.next_id);
        inner.next_id += 1;
        inner.subscribers.push((id, Rc::new(callback)));
        id
    }

    /// Register `callback` and immediately invoke it once with the current
    /// value — the common "bind a widget to state" pattern.
    pub fn bind(&self, callback: impl Fn(&T) + 'static) -> SubscriptionId {
        callback(&self.inner.borrow().value);
        self.subscribe(callback)
    }

    /// Remove a previously registered subscriber. Returns whether it was found.
    pub fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut inner = self.inner.borrow_mut();
        let before = inner.subscribers.len();
        inner.subscribers.retain(|(sid, _)| *sid != id);
        inner.subscribers.len() != before
    }

    /// Number of active subscribers (useful for leak assertions in tests).
    pub fn subscriber_count(&self) -> usize {
        self.inner.borrow().subscribers.len()
    }
}

impl<T: Clone + Default + 'static> Default for Property<T> {
    fn default() -> Self {
        Property::new(T::default())
    }
}

impl<T: Clone + fmt::Debug + 'static> fmt::Debug for Property<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Property")
            .field("value", &self.inner.borrow().value)
            .field("subscribers", &self.subscriber_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn get_returns_initial_value() {
        let p = Property::new(7);
        assert_eq!(p.get(), 7);
    }

    #[test]
    fn set_updates_value() {
        let p = Property::new(1);
        p.set(42);
        assert_eq!(p.get(), 42);
    }

    #[test]
    fn subscribe_receives_new_value() {
        let p = Property::new(0);
        let seen = Rc::new(Cell::new(0));
        let seen2 = Rc::clone(&seen);
        p.subscribe(move |v| seen2.set(*v));
        p.set(9);
        assert_eq!(seen.get(), 9);
    }

    #[test]
    fn subscribe_does_not_fire_immediately() {
        let p = Property::new(5);
        let fired = Rc::new(Cell::new(false));
        let f2 = Rc::clone(&fired);
        p.subscribe(move |_| f2.set(true));
        assert!(!fired.get());
    }

    #[test]
    fn bind_fires_immediately_with_current_value() {
        let p = Property::new(5);
        let last = Rc::new(Cell::new(0));
        let l2 = Rc::clone(&last);
        p.bind(move |v| l2.set(*v));
        assert_eq!(last.get(), 5);
        p.set(6);
        assert_eq!(last.get(), 6);
    }

    #[test]
    fn unsubscribe_stops_notifications() {
        let p = Property::new(0);
        let count = Rc::new(Cell::new(0));
        let c2 = Rc::clone(&count);
        let id = p.subscribe(move |_| c2.set(c2.get() + 1));
        p.set(1);
        assert!(p.unsubscribe(id));
        p.set(2);
        assert_eq!(count.get(), 1);
        assert!(!p.unsubscribe(id)); // already gone
    }

    #[test]
    fn update_mutates_and_notifies() {
        let p = Property::new(vec![1, 2]);
        let len_seen = Rc::new(Cell::new(0));
        let l2 = Rc::clone(&len_seen);
        p.subscribe(move |v: &Vec<i32>| l2.set(v.len()));
        p.update(|v| v.push(3));
        assert_eq!(p.get(), vec![1, 2, 3]);
        assert_eq!(len_seen.get(), 3);
    }

    #[test]
    fn set_if_changed_skips_equal_value() {
        let p = Property::new(3);
        let count = Rc::new(Cell::new(0));
        let c2 = Rc::clone(&count);
        p.subscribe(move |_| c2.set(c2.get() + 1));
        assert!(!p.set_if_changed(3));
        assert_eq!(count.get(), 0);
        assert!(p.set_if_changed(4));
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn clones_share_state() {
        let a = Property::new(0);
        let b = a.clone();
        let seen = Rc::new(Cell::new(0));
        let s2 = Rc::clone(&seen);
        b.subscribe(move |v| s2.set(*v));
        a.set(11);
        assert_eq!(b.get(), 11);
        assert_eq!(seen.get(), 11);
    }

    #[test]
    fn reentrant_set_from_subscriber_is_safe() {
        let p = Property::new(0);
        let p2 = p.clone();
        // First subscriber bumps the value once to a fixed point.
        p.subscribe(move |v| {
            if *v < 3 {
                p2.set(*v + 1);
            }
        });
        let last = Rc::new(Cell::new(0));
        let l2 = Rc::clone(&last);
        p.subscribe(move |v| l2.set(*v));
        p.set(1);
        // Re-entrancy drives it to 3 without panicking.
        assert_eq!(p.get(), 3);
        assert!(last.get() >= 1);
    }

    #[test]
    fn with_reads_without_cloning() {
        let p = Property::new(String::from("hello"));
        let len = p.with(|s| s.len());
        assert_eq!(len, 5);
    }

    #[test]
    fn subscriber_count_tracks_registrations() {
        let p = Property::new(0);
        assert_eq!(p.subscriber_count(), 0);
        let id = p.subscribe(|_| {});
        assert_eq!(p.subscriber_count(), 1);
        p.unsubscribe(id);
        assert_eq!(p.subscriber_count(), 0);
    }

    #[test]
    fn default_property_uses_type_default() {
        let p: Property<i32> = Property::default();
        assert_eq!(p.get(), 0);
    }

    #[test]
    fn debug_formats_value_and_subscribers() {
        let p = Property::new(5);
        p.subscribe(|_| {});
        let s = format!("{p:?}");
        assert!(s.contains("value"));
        assert!(s.contains("subscribers"));
    }
}
