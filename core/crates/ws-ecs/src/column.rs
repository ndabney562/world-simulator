//! Sorted component columns (ADR 0016 §2).

use core::fmt;

use crate::id::EntityId;

/// An insert found the entity already present. Nothing was changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateEntity(pub EntityId);

impl fmt::Display for DuplicateEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "entity {} is already present", self.0)
    }
}

impl std::error::Error for DuplicateEntity {}

/// The components of one type, keyed by [`EntityId`] and kept in ascending
/// ID order.
///
/// IDs and components are two parallel `Vec`s. Lookup is a binary search;
/// a single insert or remove is a binary search and a move; a batch insert
/// sorts the batch and merges in O(n + m). Every iteration is in ascending
/// ID order, and the contents depend only on the set of `(id, component)`
/// pairs, never on the order they were inserted in. Inserting an ID that is
/// already present is an error, never an overwrite. No index or handle ever
/// leaves the column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column<C> {
    ids: Vec<EntityId>,
    data: Vec<C>,
}

impl<C> Default for Column<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> Column<C> {
    /// An empty column.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ids: Vec::new(),
            data: Vec::new(),
        }
    }

    /// Number of entities with this component.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether no entity has this component.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// The IDs present, in ascending order.
    #[must_use]
    pub fn ids(&self) -> &[EntityId] {
        &self.ids
    }

    /// Whether `id` has this component.
    #[must_use]
    pub fn contains(&self, id: EntityId) -> bool {
        self.ids.binary_search(&id).is_ok()
    }

    /// The component of `id`, if present.
    #[must_use]
    pub fn get(&self, id: EntityId) -> Option<&C> {
        self.ids.binary_search(&id).ok().map(|i| &self.data[i])
    }

    /// The component of `id`, mutably, if present.
    #[must_use]
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut C> {
        self.ids.binary_search(&id).ok().map(|i| &mut self.data[i])
    }

    /// Adds the component of `id`.
    ///
    /// # Errors
    ///
    /// [`DuplicateEntity`] if `id` is already present; the column is then
    /// unchanged.
    pub fn insert(&mut self, id: EntityId, component: C) -> Result<(), DuplicateEntity> {
        match self.ids.binary_search(&id) {
            Ok(_) => Err(DuplicateEntity(id)),
            Err(i) => {
                self.ids.insert(i, id);
                self.data.insert(i, component);
                Ok(())
            }
        }
    }

    /// Removes and returns the component of `id`, if present.
    pub fn remove(&mut self, id: EntityId) -> Option<C> {
        let i = self.ids.binary_search(&id).ok()?;
        self.ids.remove(i);
        Some(self.data.remove(i))
    }

    /// Adds many components at once (materialization): the batch is sorted
    /// and merged in O(n + m). All or nothing.
    ///
    /// # Errors
    ///
    /// [`DuplicateEntity`] with the smallest offending ID if an ID appears
    /// twice in the batch or is already present; the column is then
    /// unchanged.
    pub fn insert_batch(&mut self, mut batch: Vec<(EntityId, C)>) -> Result<(), DuplicateEntity> {
        batch.sort_unstable_by_key(|(id, _)| *id);
        if let Some(w) = batch.windows(2).find(|w| w[0].0 == w[1].0) {
            return Err(DuplicateEntity(w[0].0));
        }
        if let Some((id, _)) = batch.iter().find(|(id, _)| self.contains(*id)) {
            return Err(DuplicateEntity(*id));
        }
        let old_ids = core::mem::take(&mut self.ids);
        let old_data = core::mem::take(&mut self.data);
        self.ids.reserve(old_ids.len() + batch.len());
        self.data.reserve(old_data.len() + batch.len());
        let mut old = old_ids.into_iter().zip(old_data).peekable();
        let mut new = batch.into_iter().peekable();
        loop {
            let take_old = match (old.peek(), new.peek()) {
                (Some((a, _)), Some((b, _))) => a < b,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            let (id, c) = if take_old { old.next() } else { new.next() }.expect("peeked");
            self.ids.push(id);
            self.data.push(c);
        }
        Ok(())
    }

    /// Removes the components of every ID in `ids` that is present
    /// (dematerialization) and returns them in ascending ID order. IDs that
    /// are absent are ignored.
    pub fn remove_batch(&mut self, ids: &[EntityId]) -> Vec<(EntityId, C)> {
        let mut doomed: Vec<EntityId> = ids.to_vec();
        doomed.sort_unstable();
        doomed.dedup();
        let old_ids = core::mem::take(&mut self.ids);
        let old_data = core::mem::take(&mut self.data);
        let mut removed = Vec::new();
        let mut d = doomed.iter().peekable();
        for (id, c) in old_ids.into_iter().zip(old_data) {
            while d.next_if(|x| **x < id).is_some() {}
            if d.next_if(|x| **x == id).is_some() {
                removed.push((id, c));
            } else {
                self.ids.push(id);
                self.data.push(c);
            }
        }
        removed
    }

    /// Every `(id, component)` in ascending ID order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (EntityId, &C)> + '_ {
        self.ids.iter().copied().zip(self.data.iter())
    }

    /// Every `(id, component)` in ascending ID order, mutably. Components
    /// can be changed; IDs cannot.
    pub fn iter_mut(&mut self) -> impl ExactSizeIterator<Item = (EntityId, &mut C)> + '_ {
        self.ids.iter().copied().zip(self.data.iter_mut())
    }
}

/// The entities present in both columns, with both components, in ascending
/// ID order (a merge-join over the two sorted ID lists).
pub fn join<'a, A, B>(
    a: &'a Column<A>,
    b: &'a Column<B>,
) -> impl Iterator<Item = (EntityId, &'a A, &'a B)> + 'a {
    let mut bi = b.iter().peekable();
    a.iter().filter_map(move |(id, ca)| {
        while bi.next_if(|(x, _)| *x < id).is_some() {}
        bi.next_if(|(x, _)| *x == id).map(|(_, cb)| (id, ca, cb))
    })
}

/// Calls `f` for every entity present in both columns, with both components
/// mutable, in ascending ID order.
pub fn join_mut<A, B>(
    a: &mut Column<A>,
    b: &mut Column<B>,
    mut f: impl FnMut(EntityId, &mut A, &mut B),
) {
    let mut bi = b.iter_mut().peekable();
    for (id, ca) in a.iter_mut() {
        while bi.next_if(|(x, _)| *x < id).is_some() {}
        if let Some((_, cb)) = bi.next_if(|(x, _)| *x == id) {
            f(id, ca, cb);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u64) -> EntityId {
        EntityId::procedural(n << 2)
    }

    #[test]
    fn insert_get_remove() {
        let mut c = Column::new();
        assert!(c.is_empty());
        c.insert(id(5), "five").unwrap();
        c.insert(id(1), "one").unwrap();
        c.insert(id(3), "three").unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c.get(id(3)), Some(&"three"));
        assert_eq!(c.get(id(4)), None);
        assert!(c.contains(id(1)));
        *c.get_mut(id(1)).unwrap() = "uno";
        assert_eq!(c.remove(id(1)), Some("uno"));
        assert_eq!(c.remove(id(1)), None);
        assert_eq!(c.ids(), &[id(3), id(5)]);
    }

    #[test]
    fn duplicate_insert_is_an_error_and_changes_nothing() {
        let mut c = Column::new();
        c.insert(id(2), 20).unwrap();
        assert_eq!(c.insert(id(2), 99), Err(DuplicateEntity(id(2))));
        assert_eq!(c.get(id(2)), Some(&20));
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn batch_insert_merges_and_is_all_or_nothing() {
        let mut c = Column::new();
        c.insert(id(4), 4).unwrap();
        c.insert_batch(vec![(id(9), 9), (id(1), 1), (id(6), 6)])
            .unwrap();
        assert_eq!(c.ids(), &[id(1), id(4), id(6), id(9)]);
        let before = c.clone();
        assert_eq!(
            c.insert_batch(vec![(id(2), 2), (id(6), 60)]),
            Err(DuplicateEntity(id(6)))
        );
        assert_eq!(
            c.insert_batch(vec![(id(3), 3), (id(3), 30)]),
            Err(DuplicateEntity(id(3)))
        );
        assert_eq!(c, before);
    }

    #[test]
    fn batch_remove_returns_removed_in_order() {
        let mut c = Column::new();
        c.insert_batch((1..=6).map(|n| (id(n), n)).collect())
            .unwrap();
        let removed = c.remove_batch(&[id(5), id(2), id(7), id(2)]);
        assert_eq!(removed, vec![(id(2), 2), (id(5), 5)]);
        assert_eq!(c.ids(), &[id(1), id(3), id(4), id(6)]);
    }

    #[test]
    fn join_visits_common_ids_in_order() {
        let mut a = Column::new();
        let mut b = Column::new();
        a.insert_batch([1, 2, 4, 7, 9].map(|n| (id(n), n)).to_vec())
            .unwrap();
        b.insert_batch([0, 2, 3, 7, 9, 10].map(|n| (id(n), n * 10)).to_vec())
            .unwrap();
        let got: Vec<_> = join(&a, &b).map(|(i, x, y)| (i, *x, *y)).collect();
        assert_eq!(got, vec![(id(2), 2, 20), (id(7), 7, 70), (id(9), 9, 90)]);
        let mut seen = Vec::new();
        join_mut(&mut a, &mut b, |i, x, y| {
            *x += 1;
            *y += 1;
            seen.push(i);
        });
        assert_eq!(seen, vec![id(2), id(7), id(9)]);
        assert_eq!(a.get(id(7)), Some(&8));
        assert_eq!(b.get(id(7)), Some(&71));
        assert_eq!(
            a.get(id(1)),
            Some(&1),
            "entities only in one column are untouched"
        );
    }
}
