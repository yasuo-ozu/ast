//! The three slot views, and which std types get which.
//!
//! [`SeqView`]/[`OptView`] are the **edit** halves and carry no borrow: positional, so a view stays
//! usable across a `push`, a `remove` and a walk in any order. [`View`] is the **iteration** half and
//! carries its borrow as a trait argument, so every container names its own iterators.
//!
//! All three are bare-element: `Vec<T>: SeqView<'a, T>`, `Option<T>: OptView<'a, T>`. A transparent
//! single-slot wrapper (`Box<T>`) is a `Walk` (always one node), so a `Vec<Box<T>>` is
//! `SeqView<'a, Box<T>>` — the visitor descends the box as a further `Walk` level.

use std::collections::{BTreeMap, HashMap, LinkedList, VecDeque};
use visiting::{Elements, OptEdit, OptView, SeqEdit, SeqView};

// ---- `View`: every std shape that can hand out `&mut` to what it holds ----

#[test]
fn view_covers_the_growable_sequences() {
    let mut v = vec![1, 2, 3];
    assert_eq!(v.elements().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
    for x in v.elements_mut() {
        *x *= 10;
    }
    assert_eq!(v, vec![10, 20, 30]);

    let mut d: VecDeque<i32> = vec![1, 2].into();
    assert_eq!(d.elements().copied().collect::<Vec<_>>(), vec![1, 2]);
    for x in d.elements_mut() {
        *x += 1;
    }
    assert_eq!(d.elements().copied().collect::<Vec<_>>(), vec![2, 3]);

    let mut l: LinkedList<i32> = [1, 2].into_iter().collect();
    assert_eq!(l.elements().copied().collect::<Vec<_>>(), vec![1, 2]);
    for x in l.elements_mut() {
        *x += 1;
    }
    assert_eq!(l.elements().copied().collect::<Vec<_>>(), vec![2, 3]);
}

#[test]
fn view_covers_the_fixed_shapes() {
    // Neither can grow, so neither is a `SeqView` — but both iterate.
    let mut a = [1, 2, 3];
    assert_eq!(a.elements().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
    for x in a.elements_mut() {
        *x *= 2;
    }
    assert_eq!(a, [2, 4, 6]);

    let s: &mut [i32] = &mut a;
    for x in s.elements_mut() {
        *x += 1;
    }
    assert_eq!(a, [3, 5, 7]);
}

#[test]
fn view_covers_option_as_a_zero_or_one_slot() {
    let some: Option<i32> = Some(5);
    let none: Option<i32> = None;
    assert_eq!(some.elements().count(), 1);
    assert_eq!(none.elements().count(), 0);

    let mut o: Option<i32> = Some(1);
    for x in o.elements_mut() {
        *x += 100;
    }
    assert_eq!(o, Some(101));
}

#[test]
fn view_covers_the_maps_by_value() {
    // The node sits in the VALUE slot, so that is what a map iterates. Keys are untouched.
    let mut b: BTreeMap<&str, i32> = [("a", 1), ("b", 2)].into_iter().collect();
    assert_eq!(b.elements().copied().collect::<Vec<_>>(), vec![1, 2]);
    for v in b.elements_mut() {
        *v *= 10;
    }
    assert_eq!(b["a"], 10);

    let mut h: HashMap<&str, i32> = [("a", 1)].into_iter().collect();
    for v in h.elements_mut() {
        *v += 1;
    }
    assert_eq!(h["a"], 2);
}

// A caller generic over the trait: the iterator comes back as an associated type, so this needs no
// `Box` and no `dyn`.
fn total<'a, V: Elements<'a, i32>>(c: &'a V) -> i32 {
    c.elements().copied().sum()
}

#[test]
fn a_generic_caller_names_the_iterator_as_an_associated_type() {
    assert_eq!(total(&vec![1, 2, 3]), 6);
    assert_eq!(total(&Some(4)), 4);
    assert_eq!(total(&BTreeMap::from([("a", 5)])), 5);
}

// ---- `SeqView`: positional edits, no borrow ----

#[test]
fn retain_mut_visits_then_drops() {
    let mut v = vec![1, 2, 3, 4];
    <Vec<i32> as SeqEdit<'_, i32>>::retain_mut(&mut v, |x| {
        *x += 1;
        *x % 2 == 0
    });
    assert_eq!(v, vec![2, 4], "1->2 keep, 2->3 drop, 3->4 keep, 4->5 drop");
}

#[test]
fn structural_index_ops() {
    let mut v = vec![1, 3];
    <Vec<i32> as SeqEdit<'_, i32>>::insert(&mut v, 1, 2);
    assert_eq!(v, vec![1, 2, 3]);
    assert_eq!(<Vec<i32> as SeqEdit<'_, i32>>::remove(&mut v, 0), 1);
    <Vec<i32> as SeqEdit<'_, i32>>::push(&mut v, 9);
    assert_eq!(v, vec![2, 3, 9]);
}

// The point of keeping the borrow off `SeqView`: a generic caller can read the slot, edit it, and
// read it again, in one function, without the borrow checker objecting.
fn edit_in_any_order<'a, V: SeqEdit<'a, i32>>(v: &'a mut V) {
    v.push(4);
    for i in 0..v.len() {
        v[i] *= 2;
    }
    v.retain_mut(|x| *x != 4);
    v.insert(0, 0);
}

#[test]
fn a_generic_caller_edits_and_reads_in_one_borrow() {
    let mut v = vec![1, 2, 3];
    edit_in_any_order(&mut v);
    assert_eq!(v, vec![0, 2, 6, 8], "2 was doubled to 4 and dropped");

    let mut d: VecDeque<i32> = vec![1, 2, 3].into();
    edit_in_any_order(&mut d);
    assert_eq!(d.elements().copied().collect::<Vec<_>>(), vec![0, 2, 6, 8]);
}

#[test]
fn vec_of_box_is_bare_element() {
    // Bare-element: `Vec<Box<T>>: SeqView<'_, Box<T>>` yields `&mut Box<T>` (the element is the box). The
    // visitor descends the box as a further `Walk` level; here we edit through the box directly.
    let mut v: Vec<Box<i32>> = vec![Box::new(1), Box::new(2)];
    for x in v.elements_mut() {
        **x += 100;
    }
    assert_eq!(v.iter().map(|b| **b).collect::<Vec<_>>(), vec![101, 102]);
}

#[test]
fn seqview_covers_the_fixed_size_shapes() {
    // `SeqView` without `SeqEdit`: readable, indexable, walkable — but it cannot grow.
    fn sum<'a, V: SeqView<'a, i32> + ?Sized>(v: &'a V) -> i32 {
        (0..v.len()).map(|i| v[i]).sum()
    }
    let a = [1, 2, 3];
    assert_eq!(sum(&a), 6);
    let s: &[i32] = &a;
    assert_eq!(sum(s), 6);
    assert_eq!(sum(&vec![1, 2, 3]), 6);

    let mut a = [1, 2, 3];
    a[0] = 10; // through `IndexMut`, the supertrait
    assert_eq!(<[i32; 3] as SeqView<'_, i32>>::len(&a), 3);
}

// The shape a generated `visit_<t>_seq`/`_seq_mut` has: the two traits together, over the lifetime
// of the slot the method was handed. It is what makes the container's own iterator reachable from
// inside a visitor override, and it works for a non-`'static` element — see `borrowed_elements`.
#[test]
fn the_two_traits_together_iterate_and_edit_a_view() {
    fn sum<'a, V: SeqView<'a, i32> + ?Sized>(v: &'a V) -> i32 {
        v.elements().sum()
    }
    fn bump_then_grow<'a, V: SeqEdit<'a, i32>>(v: &'a mut V) {
        // `elements_mut` takes the whole `'a` borrow, so a body that also resizes indexes instead.
        for i in 0..v.len() {
            v[i] += 1;
        }
        v.push(0);
    }

    let mut v = vec![1, 2, 3];
    assert_eq!(sum(&v), 6);
    bump_then_grow(&mut v);
    assert_eq!(v, vec![2, 3, 4, 0]);

    let d: VecDeque<i32> = vec![1, 2].into();
    assert_eq!(sum(&d), 3);
    let a: &[i32] = &[4, 5];
    assert_eq!(
        sum(a),
        9,
        "and over a fixed-size slot, which is `SeqView` but not `SeqEdit`"
    );
}

// ---- `OptView` / `OptEdit` ----

#[test]
fn optview_fills_and_empties_a_slot() {
    let mut o: Option<i32> = None;
    assert!(o.is_none());
    o.set(1);
    assert_eq!(o.get(), Some(&1));
    if let Some(x) = o.get_mut() {
        *x += 1;
    }
    assert_eq!(o.take(), Some(2));
    o.set(9);
    o.clear();
    // named through the trait: `Option` has an inherent `is_none` that would otherwise win
    // resolution, leaving `OptView`'s defaulted one untested.
    assert!(OptView::is_none(&o));
}

// `push`/`retain_mut` are inherent *on `Vec`*, so they win resolution over the same-named `SeqEdit`
// methods when `SeqEdit` is imported. `Elements`'s iterators are `elements`/`elements_mut` precisely
// so they never shadow the slice `iter`/`iter_mut` reached via `Deref` — a bare `vec.iter()` with the
// traits in scope still hits the std slice method.
#[test]
fn push_and_retain_mut_prefer_inherent_over_seqview() {
    let mut v: Vec<i32> = vec![1, 2, 3];
    v.push(4); // inherent Vec::push (SeqView::push exists but inherent wins)
    v.retain_mut(|x| *x % 2 == 0); // inherent Vec::retain_mut
    assert_eq!(
        v,
        vec![2, 4],
        "inherent methods applied, no silent trait fallback"
    );
    assert_eq!(
        v.iter().copied().sum::<i32>(),
        6,
        "slice `.iter()` unshadowed by the views"
    );
    assert_eq!(<Vec<i32> as SeqView<'_, i32>>::len(&v), 2); // trait still reachable via UFCS
}

// What making `Elements` a supertrait cost: a view is no longer *usefully* object-safe. `dyn` has to
// name every associated type in the chain, and those are the container's own iterators — so the
// erased type is right back in the annotation, which is the one thing `dyn` was for:
//
//     let _: &dyn SeqView<'_, i32> = &v;
//     error[E0191]: the value of the associated types `IterMut`, `Iter` in `Elements<'_, i32>`
//                   must be specified
//
// Spelled out it still works, which is what this pins — the traits are object-*safe*, just not
// object-*useful*. Nothing in the crate or in generated code needs it; the views reach a visitor as
// a generic parameter, never as a `dyn`.
#[test]
fn a_dyn_view_now_has_to_pin_its_iterators() {
    let mut v: Vec<i32> = vec![1, 2, 3];

    let r: &dyn SeqView<i32, Iter = core::slice::Iter<i32>, IterMut = core::slice::IterMut<i32>> =
        &v;
    assert_eq!(r.len(), 3);
    assert_eq!(r[1], 2);

    let w: &mut dyn SeqEdit<
        i32,
        Iter = core::slice::Iter<i32>,
        IterMut = core::slice::IterMut<i32>,
    > = &mut v;
    w.push(4);
    w.insert(0, 0);
    assert_eq!(w.remove(1), 1);
    assert_eq!(w.len(), 4);
    w[0] = 9;
    assert_eq!(v, vec![9, 2, 3, 4]);

    let mut o: Option<i32> = None;
    let e: &mut dyn OptEdit<
        i32,
        Iter = core::option::Iter<i32>,
        IterMut = core::option::IterMut<i32>,
    > = &mut o;
    e.set(1);
    assert_eq!(e.get(), Some(&1));
    assert_eq!(e.take(), Some(1));
    assert!(e.is_none());
}

// The provided `retain_mut` is quadratic (it can only shrink through `remove`), and a generated
// `visit_<t>_seq_mut` override always reaches the *trait* method, never an inherent one — so the
// array-backed containers forward to their own. This pins the behaviour; the cost is in the doc.
#[test]
fn retain_mut_and_push_go_through_the_containers_own() {
    fn edit<'a, V: SeqEdit<'a, i32>>(v: &'a mut V) {
        v.retain_mut(|x| {
            *x *= 10;
            *x != 20
        });
        v.push(7);
    }

    let mut v = vec![1, 2, 3];
    edit(&mut v);
    assert_eq!(
        v,
        vec![10, 30, 7],
        "every element seen, the 2 dropped, 7 appended"
    );

    let mut d: VecDeque<i32> = vec![1, 2, 3].into();
    edit(&mut d);
    assert_eq!(d, VecDeque::from(vec![10, 30, 7]));
}
