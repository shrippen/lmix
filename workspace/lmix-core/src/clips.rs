//! When a clip ends (AUT.7 `clip_end`): from the media players' position
//! reports (strom, every 200 ms) tell when the last item of a playlist
//! ends and how much of it is left.

use std::collections::HashMap;

use strom_types::FlowId;

/// One position report of a media player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerProgress {
    pub flow: FlowId,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// 0-based item of the playlist, and how many there are.
    pub index: usize,
    pub items: usize,
}

/// What one report shows that the one before did not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClipChange {
    /// The last item ended (the playlist started again).
    pub ended: bool,
    /// Time left of the last item, before and now, while it plays on.
    pub left_ms: Option<(u64, u64)>,
    /// Position in the item, before and now, while one item plays on
    /// forward.
    pub played_ms: Option<(u64, u64)>,
}

impl ClipChange {
    /// Whether a `play_time` trigger at `at_ms` fires.
    pub fn reaches(&self, at_ms: u64) -> bool {
        self.played_ms
            .is_some_and(|(was, now)| was < at_ms && now >= at_ms)
    }

    /// Whether a countdown trigger at `before_ms` fires.
    pub fn crosses(&self, before_ms: u64) -> bool {
        self.left_ms
            .is_some_and(|(was, now)| was > before_ms && now <= before_ms)
    }
}

/// A position report counts as the start again when it is this far before
/// the last one (a looping single file).
const JUMP_BACK_MS: u64 = 1_000;

/// The last report per player.
#[derive(Default)]
pub struct ClipWatch {
    last: HashMap<FlowId, PlayerProgress>,
}

impl ClipWatch {
    pub fn update(&mut self, now: PlayerProgress) -> ClipChange {
        let Some(before) = self.last.insert(now.flow, now) else {
            return ClipChange::default();
        };
        let last_item = |p: &PlayerProgress| p.index + 1 >= p.items.max(1);
        let left = |p: &PlayerProgress| p.duration_ms.saturating_sub(p.position_ms);
        let ended = last_item(&before)
            && if now.items > 1 {
                now.index < before.index
            } else {
                now.position_ms + JUMP_BACK_MS < before.position_ms
            };
        let same_item = now.index == before.index && !ended;
        let left_ms = (same_item && last_item(&now) && now.duration_ms > 0)
            .then(|| (left(&before), left(&now)));
        // A new item, or the same again (loop, restart), plays from its
        // start: times since then are reached too. No firing on the way
        // back within an item.
        let played_ms = if same_item && now.position_ms + JUMP_BACK_MS >= before.position_ms {
            Some((before.position_ms, now.position_ms.max(before.position_ms)))
        } else {
            Some((0, now.position_ms))
        };
        ClipChange {
            ended,
            left_ms,
            played_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(index: usize, items: usize, position_ms: u64) -> PlayerProgress {
        PlayerProgress {
            flow: FlowId::nil(),
            position_ms,
            duration_ms: 10_000,
            index,
            items,
        }
    }

    #[test]
    fn playlist_end_and_countdown() {
        let mut watch = ClipWatch::default();
        assert_eq!(watch.update(at(0, 2, 9_900)), ClipChange::default());
        // Moving on to the last item is no end.
        assert!(!watch.update(at(1, 2, 100)).ended);
        let change = watch.update(at(1, 2, 7_100));
        assert!(change.crosses(3_000), "{change:?}");
        assert!(!watch.update(at(1, 2, 7_300)).crosses(3_000), "fires once");
        assert!(watch.update(at(0, 2, 150)).ended);
    }

    #[test]
    fn playing_time_is_reached_once_per_pass() {
        let mut watch = ClipWatch::default();
        watch.update(at(0, 1, 4_900));
        assert!(watch.update(at(0, 1, 5_100)).reaches(5_000));
        assert!(!watch.update(at(0, 1, 5_300)).reaches(5_000), "fires once");
        // Looped: reached again on the next pass, not on the jump back.
        assert!(!watch.update(at(0, 1, 100)).reaches(5_000));
        watch.update(at(0, 1, 4_950));
        assert!(watch.update(at(0, 1, 5_000)).reaches(5_000));
        // Early times fire on the report after the start, too.
        assert!(watch.update(at(0, 1, 150)).reaches(100));
        // A new item starts its own time.
        let mut list = ClipWatch::default();
        list.update(at(0, 2, 9_900));
        assert!(list.update(at(1, 2, 180)).reaches(100));
        assert!(!list.update(at(1, 2, 300)).reaches(100), "once");
    }

    #[test]
    fn a_looping_file_ends_when_it_starts_again() {
        let mut watch = ClipWatch::default();
        watch.update(at(0, 1, 9_800));
        let change = watch.update(at(0, 1, 120));
        assert!(change.ended);
        assert!(!change.crosses(500), "the jump back is no countdown");
        assert!(!watch.update(at(0, 1, 400)).ended);
    }
}
