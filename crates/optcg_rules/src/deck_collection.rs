use serde::{Deserialize, Serialize};
use std::path::Path;

/// Current on-disk schema version for a persisted collection.
pub const COLLECTION_VERSION: u32 = 1;

/// Upper bound on saved decks, so a runaway caller cannot grow the file forever.
pub const MAX_DECKS: usize = 50;

/// One deck the user has saved for reuse across sessions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedDeck {
    /// Stable slug derived from the name at creation time.
    pub id: String,
    pub name: String,
    /// Original pasted text, re-parsed on activation so card DB updates apply.
    pub raw: String,
    pub leader_id: Option<String>,
    pub total_cards: u32,
    pub created_at: String,
    pub updated_at: String,
}

/// The user's saved deck library plus which deck is currently active.
///
/// `decks` is kept in most-recently-edited-first order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckCollection {
    pub version: u32,
    pub decks: Vec<SavedDeck>,
    /// The list attached to your side, or `None` to read it from play.
    pub active_id: Option<String>,
    /// The list attached to the opponent's side, or `None` to read it from
    /// play. Defaulted so collections written before this existed still load.
    #[serde(default)]
    pub opponent_id: Option<String>,
}

impl Default for DeckCollection {
    fn default() -> Self {
        Self {
            version: COLLECTION_VERSION,
            decks: Vec::new(),
            active_id: None,
            opponent_id: None,
        }
    }
}

/// Which side of the board a saved list is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    You,
    Opponent,
}

impl Side {
    pub fn other(self) -> Self {
        match self {
            Self::You => Self::Opponent,
            Self::Opponent => Self::You,
        }
    }
}

impl DeckCollection {
    /// Read a collection from disk, falling back to an empty one when the file
    /// is missing or unreadable. A corrupt file must not stop the HUD booting.
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Self>(&raw) {
            Ok(mut collection) => {
                collection.version = COLLECTION_VERSION;
                collection.prune();
                collection
            }
            Err(e) => {
                tracing::warn!(error = %e, path = %path.display(), "deck collection unreadable; starting empty");
                Self::default()
            }
        }
    }

    /// Write the collection out via a temp file + rename, so a crash mid-write
    /// cannot truncate an existing good collection.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    pub fn is_empty(&self) -> bool {
        self.decks.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&SavedDeck> {
        self.decks.iter().find(|d| d.id == id)
    }

    /// The active deck, or `None` when nothing is selected.
    pub fn active(&self) -> Option<&SavedDeck> {
        self.active_id.as_ref().and_then(|id| self.get(id))
    }

    /// Select `id` as active. Returns false when no such deck exists.
    pub fn set_active(&mut self, id: &str) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        self.active_id = Some(id.to_string());
        true
    }

    fn assignment(&self, side: Side) -> &Option<String> {
        match side {
            Side::You => &self.active_id,
            Side::Opponent => &self.opponent_id,
        }
    }

    fn assignment_mut(&mut self, side: Side) -> &mut Option<String> {
        match side {
            Side::You => &mut self.active_id,
            Side::Opponent => &mut self.opponent_id,
        }
    }

    /// The list attached to `side`, or `None` when that side reads from play.
    pub fn attached(&self, side: Side) -> Option<&SavedDeck> {
        self.assignment(side).as_ref().and_then(|id| self.get(id))
    }

    /// Attach a saved list to `side`, or pass `None` to read from play instead.
    ///
    /// Returns false only when `id` names a deck that does not exist, so a
    /// stale id from the UI cannot silently detach the side.
    pub fn attach(&mut self, side: Side, id: Option<&str>) -> bool {
        match id {
            Some(id) => {
                if self.get(id).is_none() {
                    return false;
                }
                *self.assignment_mut(side) = Some(id.to_string());
            }
            None => *self.assignment_mut(side) = None,
        }
        true
    }

    /// The list to presume for `side` given the leader on the table.
    ///
    /// This is what lets a side be read from play: recognise the leader and the
    /// list saved for it comes back. Two rules keep it from inventing decks.
    /// Several lists on one leader means declining, since choosing between them
    /// would put cards in front of the coach that nobody is playing. And a list
    /// attached to the other side is skipped, because sharing a leader in a
    /// mirror match is no reason to hand your own fifty cards to the opponent.
    pub fn presumed_for(&self, side: Side, leader_id: &str) -> Option<&SavedDeck> {
        if leader_id.is_empty() {
            return None;
        }
        let taken = self.assignment(side.other()).as_deref();
        let mut matches = self.decks.iter().filter(|deck| {
            deck.leader_id.as_deref() == Some(leader_id) && Some(deck.id.as_str()) != taken
        });
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    /// Insert a deck, or replace the contents of an existing one.
    ///
    /// Passing `id` targets that specific deck; passing `None` matches an
    /// existing deck by name so re-pasting the same list updates it in place
    /// instead of piling up duplicates. The saved deck becomes active.
    pub fn upsert(
        &mut self,
        id: Option<&str>,
        name: &str,
        raw: &str,
        leader_id: Option<String>,
        total_cards: u32,
        now: &str,
    ) -> Result<String, String> {
        let name = normalize_name(name);

        let existing = match id {
            Some(id) => {
                if self.get(id).is_none() {
                    return Err(format!("No saved deck with id '{id}'"));
                }
                Some(id.to_string())
            }
            None => self
                .decks
                .iter()
                .find(|d| d.name.eq_ignore_ascii_case(&name))
                .map(|d| d.id.clone()),
        };

        let deck_id = match existing {
            Some(existing_id) => {
                let deck = self
                    .decks
                    .iter_mut()
                    .find(|d| d.id == existing_id)
                    .expect("existence checked above");
                deck.name = name;
                deck.raw = raw.to_string();
                deck.leader_id = leader_id;
                deck.total_cards = total_cards;
                deck.updated_at = now.to_string();
                existing_id
            }
            None => {
                if self.decks.len() >= MAX_DECKS {
                    return Err(format!("Deck collection is full ({MAX_DECKS} decks)"));
                }
                let new_id = self.unique_id(&name);
                self.decks.push(SavedDeck {
                    id: new_id.clone(),
                    name,
                    raw: raw.to_string(),
                    leader_id,
                    total_cards,
                    created_at: now.to_string(),
                    updated_at: now.to_string(),
                });
                new_id
            }
        };

        self.touch(&deck_id);
        self.active_id = Some(deck_id.clone());
        Ok(deck_id)
    }

    /// Delete a deck. When it was active, the next most recent deck takes over
    /// so the HUD is never left pointing at a deck that no longer exists.
    pub fn remove(&mut self, id: &str) -> bool {
        let Some(pos) = self.decks.iter().position(|d| d.id == id) else {
            return false;
        };
        self.decks.remove(pos);
        if self.active_id.as_deref() == Some(id) {
            self.active_id = self.decks.first().map(|d| d.id.clone());
        }
        // The opponent falls back to being read from play rather than to some
        // arbitrary other list, which would be a claim about their deck.
        if self.opponent_id.as_deref() == Some(id) {
            self.opponent_id = None;
        }
        true
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), String> {
        let name = normalize_name(name);
        if self
            .decks
            .iter()
            .any(|d| d.id != id && d.name.eq_ignore_ascii_case(&name))
        {
            return Err(format!("A deck named '{name}' already exists"));
        }
        match self.decks.iter_mut().find(|d| d.id == id) {
            Some(deck) => {
                deck.name = name;
                Ok(())
            }
            None => Err(format!("No saved deck with id '{id}'")),
        }
    }

    /// Move a deck to the front of the most-recently-edited ordering.
    fn touch(&mut self, id: &str) {
        if let Some(pos) = self.decks.iter().position(|d| d.id == id) {
            let deck = self.decks.remove(pos);
            self.decks.insert(0, deck);
        }
    }

    /// Drop anything a hand-edited or corrupt file could have introduced:
    /// duplicate ids, over-cap decks, and a dangling active pointer.
    fn prune(&mut self) {
        let mut seen: Vec<String> = Vec::new();
        self.decks.retain(|deck| {
            if deck.id.trim().is_empty() || seen.iter().any(|s| s == &deck.id) {
                return false;
            }
            seen.push(deck.id.clone());
            true
        });
        self.decks.truncate(MAX_DECKS);
        if let Some(active) = self.active_id.clone() {
            if !self.decks.iter().any(|d| d.id == active) {
                self.active_id = self.decks.first().map(|d| d.id.clone());
            }
        }
        if let Some(opponent) = self.opponent_id.clone() {
            if !self.decks.iter().any(|d| d.id == opponent) {
                self.opponent_id = None;
            }
        }
    }

    fn unique_id(&self, name: &str) -> String {
        let base = slugify(name);
        if !self.decks.iter().any(|d| d.id == base) {
            return base;
        }
        // `MAX_DECKS + 2` guarantees a free suffix even at capacity.
        (2..=MAX_DECKS + 2)
            .map(|n| format!("{base}-{n}"))
            .find(|candidate| !self.decks.iter().any(|d| &d.id == candidate))
            .unwrap_or(base)
    }
}

fn normalize_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "Untitled deck".to_string()
    } else {
        trimmed.chars().take(60).collect()
    }
}

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for ch in name.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let slug: String = slug.chars().take(40).collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "deck".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-01-01T00:00:00Z";
    const LATER: &str = "2026-01-02T00:00:00Z";

    fn add(collection: &mut DeckCollection, name: &str, raw: &str) -> String {
        collection
            .upsert(None, name, raw, None, 50, NOW)
            .expect("upsert should succeed")
    }

    fn add_with_leader(collection: &mut DeckCollection, name: &str, leader: &str) -> String {
        collection
            .upsert(None, name, "4x ST01-002", Some(leader.to_string()), 50, NOW)
            .expect("upsert should succeed")
    }

    #[test]
    fn both_sides_read_from_play_until_a_list_is_attached() {
        let mut collection = DeckCollection::default();
        add(&mut collection, "Red Luffy Aggro", "4x ST01-002");

        assert!(
            collection.attached(Side::Opponent).is_none(),
            "the opponent is never assumed to be playing a list you saved"
        );
    }

    #[test]
    fn a_list_attaches_to_the_opponent_without_touching_your_side() {
        let mut collection = DeckCollection::default();
        let yours = add(&mut collection, "Red Luffy Aggro", "4x ST01-002");
        let theirs = add(&mut collection, "Blue Doflamingo", "4x OP01-060");
        collection.attach(Side::You, Some(&yours));

        assert!(collection.attach(Side::Opponent, Some(&theirs)));
        assert_eq!(
            collection.attached(Side::You).map(|d| d.id.as_str()),
            Some(yours.as_str())
        );
        assert_eq!(
            collection.attached(Side::Opponent).map(|d| d.id.as_str()),
            Some(theirs.as_str())
        );
    }

    #[test]
    fn detaching_a_side_returns_it_to_being_read_from_play() {
        let mut collection = DeckCollection::default();
        let id = add(&mut collection, "Blue Doflamingo", "4x OP01-060");
        collection.attach(Side::Opponent, Some(&id));

        assert!(collection.attach(Side::Opponent, None));
        assert!(collection.attached(Side::Opponent).is_none());
    }

    #[test]
    fn attaching_an_unknown_list_is_refused_rather_than_detaching() {
        let mut collection = DeckCollection::default();
        let id = add(&mut collection, "Blue Doflamingo", "4x OP01-060");
        collection.attach(Side::Opponent, Some(&id));

        assert!(!collection.attach(Side::Opponent, Some("no-such-deck")));
        assert_eq!(
            collection.attached(Side::Opponent).map(|d| d.id.as_str()),
            Some(id.as_str()),
            "a stale id must not quietly clear the side"
        );
    }

    #[test]
    fn a_leader_seen_across_the_table_finds_the_list_you_saved_for_it() {
        let mut collection = DeckCollection::default();
        let id = add_with_leader(&mut collection, "Blue Doflamingo", "OP01-060");
        collection.attach(Side::You, None);

        assert_eq!(
            collection
                .presumed_for(Side::Opponent, "OP01-060")
                .map(|d| d.id.as_str()),
            Some(id.as_str())
        );
    }

    #[test]
    fn a_leader_with_two_saved_lists_is_left_unresolved() {
        let mut collection = DeckCollection::default();
        add_with_leader(&mut collection, "Doflamingo Control", "OP01-060");
        add_with_leader(&mut collection, "Doflamingo Ramp", "OP01-060");
        collection.attach(Side::You, None);

        assert!(
            collection
                .presumed_for(Side::Opponent, "OP01-060")
                .is_none(),
            "picking between two lists would invent cards the opponent may not hold"
        );
    }

    #[test]
    fn an_unseen_leader_matches_nothing() {
        let mut collection = DeckCollection::default();
        add_with_leader(&mut collection, "Blue Doflamingo", "OP01-060");

        assert!(collection
            .presumed_for(Side::Opponent, "ST01-001")
            .is_none());
        assert!(
            collection.presumed_for(Side::Opponent, "").is_none(),
            "an unread leader must not match the first saved list"
        );
    }

    #[test]
    fn a_mirror_match_does_not_hand_the_opponent_your_own_list() {
        let mut collection = DeckCollection::default();
        let yours = add_with_leader(&mut collection, "My Doflamingo", "OP01-060");
        collection.attach(Side::You, Some(&yours));

        assert!(
            collection
                .presumed_for(Side::Opponent, "OP01-060")
                .is_none(),
            "sharing a leader is not evidence they are on your list"
        );
        assert_eq!(
            collection
                .presumed_for(Side::You, "OP01-060")
                .map(|d| d.id.as_str()),
            Some(yours.as_str()),
            "your own side should still recognise it"
        );
    }

    #[test]
    fn deleting_the_opponents_list_returns_them_to_being_read_from_play() {
        let mut collection = DeckCollection::default();
        add(&mut collection, "Red Luffy Aggro", "4x ST01-002");
        let theirs = add(&mut collection, "Blue Doflamingo", "4x OP01-060");
        collection.attach(Side::Opponent, Some(&theirs));

        assert!(collection.remove(&theirs));
        assert!(
            collection.opponent_id.is_none(),
            "the opponent must not inherit some other saved list"
        );
    }

    #[test]
    fn upsert_creates_slug_id_and_activates() {
        let mut collection = DeckCollection::default();
        let id = add(&mut collection, "Red Luffy Aggro", "4x ST01-002");

        assert_eq!(id, "red-luffy-aggro");
        assert_eq!(collection.decks.len(), 1);
        assert_eq!(collection.active_id.as_deref(), Some("red-luffy-aggro"));
        assert_eq!(
            collection.active().map(|d| d.name.as_str()),
            Some("Red Luffy Aggro")
        );
    }

    #[test]
    fn upsert_by_same_name_updates_in_place() {
        let mut collection = DeckCollection::default();
        add(&mut collection, "Red Luffy", "4x ST01-002");
        let id = collection
            .upsert(
                None,
                "red luffy",
                "4x ST01-003",
                Some("ST01-001".into()),
                51,
                LATER,
            )
            .unwrap();

        assert_eq!(collection.decks.len(), 1, "same name must not duplicate");
        assert_eq!(id, "red-luffy");
        let deck = collection.get(&id).unwrap();
        assert_eq!(deck.raw, "4x ST01-003");
        assert_eq!(deck.total_cards, 51);
        assert_eq!(deck.leader_id.as_deref(), Some("ST01-001"));
        assert_eq!(deck.created_at, NOW, "creation time is preserved");
        assert_eq!(deck.updated_at, LATER);
    }

    #[test]
    fn distinct_names_that_slug_alike_get_distinct_ids() {
        let mut collection = DeckCollection::default();
        let first = add(&mut collection, "Red Luffy!", "a");
        let second = add(&mut collection, "Red  Luffy?", "b");

        assert_eq!(first, "red-luffy");
        assert_eq!(second, "red-luffy-2");
        assert_eq!(collection.decks.len(), 2);
    }

    #[test]
    fn edited_deck_moves_to_front() {
        let mut collection = DeckCollection::default();
        let first = add(&mut collection, "Deck A", "a");
        add(&mut collection, "Deck B", "b");
        assert_eq!(collection.decks[0].id, "deck-b");

        collection
            .upsert(Some(&first), "Deck A", "a2", None, 50, LATER)
            .unwrap();
        assert_eq!(collection.decks[0].id, "deck-a");
    }

    #[test]
    fn upsert_with_unknown_id_is_rejected() {
        let mut collection = DeckCollection::default();
        let err = collection
            .upsert(Some("nope"), "Deck", "raw", None, 50, NOW)
            .unwrap_err();
        assert!(err.contains("nope"), "error should name the id: {err}");
        assert!(collection.is_empty());
    }

    #[test]
    fn blank_name_falls_back_to_placeholder() {
        let mut collection = DeckCollection::default();
        let id = add(&mut collection, "   ", "raw");
        assert_eq!(collection.get(&id).unwrap().name, "Untitled deck");
    }

    #[test]
    fn removing_active_deck_promotes_another() {
        let mut collection = DeckCollection::default();
        add(&mut collection, "Deck A", "a");
        let b = add(&mut collection, "Deck B", "b");
        assert_eq!(collection.active_id.as_deref(), Some(b.as_str()));

        assert!(collection.remove(&b));
        assert_eq!(collection.active_id.as_deref(), Some("deck-a"));

        assert!(collection.remove("deck-a"));
        assert_eq!(
            collection.active_id, None,
            "empty collection has no active deck"
        );
        assert!(
            !collection.remove("deck-a"),
            "second remove reports nothing removed"
        );
    }

    #[test]
    fn set_active_rejects_unknown_deck() {
        let mut collection = DeckCollection::default();
        add(&mut collection, "Deck A", "a");
        assert!(!collection.set_active("ghost"));
        assert_eq!(collection.active_id.as_deref(), Some("deck-a"));
    }

    #[test]
    fn rename_rejects_duplicate_names() {
        let mut collection = DeckCollection::default();
        let a = add(&mut collection, "Deck A", "a");
        add(&mut collection, "Deck B", "b");

        assert!(collection.rename(&a, "deck b").is_err());
        collection.rename(&a, "Deck C").unwrap();
        assert_eq!(collection.get(&a).unwrap().name, "Deck C");
        assert_eq!(
            collection.get(&a).unwrap().id,
            a,
            "rename keeps the id stable"
        );
    }

    #[test]
    fn collection_is_capped() {
        let mut collection = DeckCollection::default();
        for i in 0..MAX_DECKS {
            add(&mut collection, &format!("Deck {i}"), "raw");
        }
        let err = collection
            .upsert(None, "One too many", "raw", None, 50, NOW)
            .unwrap_err();
        assert!(err.contains("full"), "unexpected error: {err}");
        assert_eq!(collection.decks.len(), MAX_DECKS);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("optcg-collection-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deck_collection.json");

        let mut collection = DeckCollection::default();
        add(&mut collection, "Red Luffy Aggro", "4x ST01-002");
        add(&mut collection, "Black Elbaph", "4x OP01-016");
        collection.save(&path).unwrap();

        let loaded = DeckCollection::load(&path);
        assert_eq!(loaded.version, COLLECTION_VERSION);
        assert_eq!(loaded.decks, collection.decks);
        assert_eq!(loaded.active_id, collection.active_id);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_or_corrupt_file_loads_empty() {
        let dir = std::env::temp_dir().join(format!("optcg-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let missing = dir.join("absent.json");
        assert!(DeckCollection::load(&missing).is_empty());

        let corrupt = dir.join("corrupt.json");
        std::fs::write(&corrupt, "{ not json").unwrap();
        assert!(DeckCollection::load(&corrupt).is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_drops_duplicate_ids_and_dangling_active() {
        let dir = std::env::temp_dir().join(format!("optcg-prune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hand_edited.json");
        std::fs::write(
            &path,
            r#"{
              "version": 1,
              "active_id": "ghost",
              "decks": [
                {"id":"a","name":"A","raw":"x","leader_id":null,"total_cards":50,"created_at":"t","updated_at":"t"},
                {"id":"a","name":"A dup","raw":"y","leader_id":null,"total_cards":50,"created_at":"t","updated_at":"t"}
              ]
            }"#,
        )
        .unwrap();

        let loaded = DeckCollection::load(&path);
        assert_eq!(loaded.decks.len(), 1, "duplicate id dropped");
        assert_eq!(
            loaded.active_id.as_deref(),
            Some("a"),
            "dangling active repaired"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
