use crate::uax29::{Action, sentence::properties::SentenceBreakProperty, state_enum};

// State values for the sentence break state machine. These are an
// implementation detail of UAX#29, not in the spec.
state_enum! {
    StartOfText, Any, CR, ParaSep, Upper, Lower, ATerm, LetterATerm, STerm,
    ATermClose, ATermCloseSp, STermClose, STermCloseSp, SB8Pending,
}

impl State {
    /// Returns true if this state represents a deferred break, meaning that
    /// whether we break or not depends on the next character.
    pub const fn is_deferred(self) -> bool {
        matches!(self, State::SB8Pending)
    }
}

/// A transition in the sentence break state machine, which consists of a new state and an action to take.
#[derive(Clone, Copy)]
pub(crate) struct Transition(pub(crate) State, pub(crate) Action);

/// A row is a mapping from property values to transitions.
type Row = [Transition; SentenceBreakProperty::NUM_VARIANTS];

/// Defines the state machine transitions. Constructed at compile time.
pub(crate) const TRANSITION_TABLE: [Row; State::NUM_VARIANTS] = [
    start_of_text_transitions(),
    any_transitions(),
    cr_transitions(),
    parasep_transitions(),
    upper_transitions(),
    lower_transitions(),
    aterm_transitions(),
    letter_aterm_transitions(),
    sterm_transitions(),
    aterm_close_transitions(),
    aterm_close_sp_transitions(),
    sterm_close_transitions(),
    sterm_close_sp_transitions(),
    sb8_pending_transitions(),
];

const fn default_all_break() -> Row {
    let mut row = [brk(State::Any); SentenceBreakProperty::NUM_VARIANTS];
    row[SentenceBreakProperty::CR as usize] = brk(State::CR);
    row[SentenceBreakProperty::LF as usize] = brk(State::ParaSep);
    row[SentenceBreakProperty::Sep as usize] = brk(State::ParaSep);
    row[SentenceBreakProperty::Upper as usize] = brk(State::Upper);
    row[SentenceBreakProperty::Lower as usize] = brk(State::Lower);
    row[SentenceBreakProperty::ATerm as usize] = brk(State::ATerm);
    row[SentenceBreakProperty::STerm as usize] = brk(State::STerm);

    // SB5: Format and Extend characters are transparent.
    row[SentenceBreakProperty::Extend as usize] = transparent();
    row[SentenceBreakProperty::Format as usize] = transparent();
    row
}

const fn default_no_break() -> Row {
    let mut row = [nb(State::Any); SentenceBreakProperty::NUM_VARIANTS];
    row[SentenceBreakProperty::CR as usize] = nb(State::CR);
    row[SentenceBreakProperty::LF as usize] = nb(State::ParaSep);
    row[SentenceBreakProperty::Sep as usize] = nb(State::ParaSep);
    row[SentenceBreakProperty::Upper as usize] = nb(State::Upper);
    row[SentenceBreakProperty::Lower as usize] = nb(State::Lower);
    row[SentenceBreakProperty::ATerm as usize] = nb(State::ATerm);
    row[SentenceBreakProperty::STerm as usize] = nb(State::STerm);

    // SB5: Format and Extend characters are transparent.
    row[SentenceBreakProperty::Extend as usize] = transparent();
    row[SentenceBreakProperty::Format as usize] = transparent();
    row
}

/// Override Extend/Format back to Break — used by states exempt from SB5
/// (StartOfText, CR, ParaSep)
const fn without_sb5(row: &mut Row) {
    row[SentenceBreakProperty::Extend as usize] = brk(State::Any);
    row[SentenceBreakProperty::Format as usize] = brk(State::Any);
}

/// SB8a: SATerm Close* Sp* × (SContinue | SATerm)
/// Common to all SATerm-chain states.
const fn with_sb8a(row: &mut Row) {
    row[SentenceBreakProperty::SContinue as usize] = nb(State::Any);
    row[SentenceBreakProperty::ATerm as usize] = nb(State::ATerm);
    row[SentenceBreakProperty::STerm as usize] = nb(State::STerm);
}

/// SB9: SATerm Close* × (Close | Sp | ParaSep)
/// Parameterized by which Close/Sp states to use (ATerm vs STerm path).
const fn with_sb9(row: &mut Row, close_state: State, sp_state: State) {
    row[SentenceBreakProperty::Close as usize] = nb(close_state);
    row[SentenceBreakProperty::Sp as usize] = nb(sp_state);
    row[SentenceBreakProperty::CR as usize] = nb(State::CR);
    row[SentenceBreakProperty::LF as usize] = nb(State::ParaSep);
    row[SentenceBreakProperty::Sep as usize] = nb(State::ParaSep);
}

// State::StartOfText
// SB1: sot	÷ Any (break at the start of text)
const fn start_of_text_transitions() -> Row {
    let mut row = default_all_break();
    without_sb5(&mut row);
    row
}

// State::Any
// SB998: Any × Any (by default, don't break)
const fn any_transitions() -> Row {
    default_no_break()
}

// State::CR
// SB3: CR × LF (don't break between CR and LF)
const fn cr_transitions() -> Row {
    let mut row = default_all_break();
    row[SentenceBreakProperty::LF as usize] = nb(State::ParaSep);
    without_sb5(&mut row);
    row
}

// State::ParaSep
// SB4	ParaSep	÷ (break after paragraph separators)
// ParaSep = (Sep | CR | LF)
const fn parasep_transitions() -> Row {
    let mut row = default_all_break();
    without_sb5(&mut row);
    row
}

// State::Upper
// Like Any, but ATerm goes to LetterATerm (for SB7).
const fn upper_transitions() -> Row {
    let mut row = default_no_break();
    row[SentenceBreakProperty::ATerm as usize] = nb(State::LetterATerm);
    row
}

// State::Lower
// Like Any, but ATerm goes to LetterATerm (for SB7).
const fn lower_transitions() -> Row {
    let mut row = default_no_break();
    row[SentenceBreakProperty::ATerm as usize] = nb(State::LetterATerm);
    row
}

// State::ATerm
// SB6: ATerm × Numeric
// SB8: ATerm × Lower (direct match), ATerm × Other → SB8Pending (deferred)
// SB8a/SB9/SB11
const fn aterm_transitions() -> Row {
    let mut row = default_all_break();

    // SB6: ATerm × Numeric
    row[SentenceBreakProperty::Numeric as usize] = nb(State::Any);

    // SB8: ATerm × (¬(OLetter | Upper | Lower | ParaSep | SATerm))* Lower
    row[SentenceBreakProperty::Lower as usize] = nb(State::Lower);
    row[SentenceBreakProperty::Other as usize] = nb(State::SB8Pending);

    // SB8a, SB9
    with_sb8a(&mut row);
    with_sb9(&mut row, State::ATermClose, State::ATermCloseSp);

    row
}

// State::LetterATerm — ATerm preceded by (Upper | Lower)
// SB7: (Upper | Lower) ATerm × Upper
// Plus all the same rules as ATerm (SB6, SB8, SB8a, SB9, SB11).
const fn letter_aterm_transitions() -> Row {
    let mut row = aterm_transitions();

    // SB7: (Upper | Lower) ATerm × Upper
    row[SentenceBreakProperty::Upper as usize] = nb(State::Upper);

    row
}

// State::STerm
// SB8a/SB9/SB11 (no SB8 — that's ATerm-only)
const fn sterm_transitions() -> Row {
    let mut row = default_all_break();

    // SB8a, SB9
    with_sb8a(&mut row);
    with_sb9(&mut row, State::STermClose, State::STermCloseSp);

    row
}

// State::ATermClose — after ATerm Close+
// SB8: Lower direct, Other/Numeric deferred
// SB8a/SB9/SB11
const fn aterm_close_transitions() -> Row {
    let mut row = default_all_break();

    // SB8
    row[SentenceBreakProperty::Lower as usize] = nb(State::Lower);
    row[SentenceBreakProperty::Other as usize] = nb(State::SB8Pending);
    row[SentenceBreakProperty::Numeric as usize] = nb(State::SB8Pending);

    // SB8a, SB9
    with_sb8a(&mut row);
    with_sb9(&mut row, State::ATermClose, State::ATermCloseSp);

    row
}

// State::ATermCloseSp — after ATerm Close* Sp+
// SB8: Lower direct, Other/Numeric/Close deferred
// SB8a/SB10/SB11
const fn aterm_close_sp_transitions() -> Row {
    let mut row = default_all_break();

    // SB8
    row[SentenceBreakProperty::Lower as usize] = nb(State::Lower);
    row[SentenceBreakProperty::Other as usize] = nb(State::SB8Pending);
    row[SentenceBreakProperty::Numeric as usize] = nb(State::SB8Pending);
    row[SentenceBreakProperty::Close as usize] = nb(State::SB8Pending);

    // SB8a
    with_sb8a(&mut row);

    // SB10: SATerm Close* Sp* × (Sp | ParaSep)
    row[SentenceBreakProperty::Sp as usize] = nb(State::ATermCloseSp);
    row[SentenceBreakProperty::CR as usize] = nb(State::CR);
    row[SentenceBreakProperty::LF as usize] = nb(State::ParaSep);
    row[SentenceBreakProperty::Sep as usize] = nb(State::ParaSep);

    row
}

// State::STermClose — after STerm Close+
// SB8a/SB9/SB11 (no SB8)
const fn sterm_close_transitions() -> Row {
    let mut row = default_all_break();

    // SB8a, SB9
    with_sb8a(&mut row);
    with_sb9(&mut row, State::STermClose, State::STermCloseSp);

    row
}

// State::STermCloseSp — after STerm Close* Sp+
// SB8a/SB10/SB11 (no SB8)
const fn sterm_close_sp_transitions() -> Row {
    let mut row = default_all_break();

    // SB8a
    with_sb8a(&mut row);

    // SB10: SATerm Close* Sp* × (Sp | ParaSep)
    row[SentenceBreakProperty::Sp as usize] = nb(State::STermCloseSp);
    row[SentenceBreakProperty::CR as usize] = nb(State::CR);
    row[SentenceBreakProperty::LF as usize] = nb(State::ParaSep);
    row[SentenceBreakProperty::Sep as usize] = nb(State::ParaSep);

    row
}

// State::SB8Pending — deferred break for SB8
// We've seen ATerm Close* Sp* followed by one or more characters in the
// (¬(OLetter | Upper | Lower | ParaSep | SATerm))* set. If we eventually
// see Lower, SB8 matches and we don't break. Otherwise, we confirm the
// deferred break (SB11).
const fn sb8_pending_transitions() -> Row {
    // Default: SB8 failed, confirm deferred break
    let mut row = [deferred(State::Any); SentenceBreakProperty::NUM_VARIANTS];
    row[SentenceBreakProperty::CR as usize] = deferred(State::CR);
    row[SentenceBreakProperty::LF as usize] = deferred(State::ParaSep);
    row[SentenceBreakProperty::Sep as usize] = deferred(State::ParaSep);
    row[SentenceBreakProperty::Upper as usize] = deferred(State::Upper);
    row[SentenceBreakProperty::ATerm as usize] = deferred(State::ATerm);
    row[SentenceBreakProperty::STerm as usize] = deferred(State::STerm);

    // SB5: transparent
    row[SentenceBreakProperty::Extend as usize] = transparent();
    row[SentenceBreakProperty::Format as usize] = transparent();

    // SB8 succeeded: Lower found, cancel the deferred break
    row[SentenceBreakProperty::Lower as usize] = nb(State::Lower);

    // SB8 skip: keep looking for Lower
    row[SentenceBreakProperty::Other as usize] = nb(State::SB8Pending);
    row[SentenceBreakProperty::Close as usize] = nb(State::SB8Pending);
    row[SentenceBreakProperty::Numeric as usize] = nb(State::SB8Pending);
    row[SentenceBreakProperty::Sp as usize] = nb(State::SB8Pending);
    row[SentenceBreakProperty::SContinue as usize] = nb(State::SB8Pending);

    row
}

const fn brk(s: State) -> Transition {
    Transition(s, Action::Break)
}

const fn nb(s: State) -> Transition {
    Transition(s, Action::NoBreak)
}

const fn deferred(s: State) -> Transition {
    Transition(s, Action::DeferredBreak)
}

const fn transparent() -> Transition {
    Transition(State::Any, Action::Transparent)
}
