use crate::uax29::{Action, state_enum, word::properties::WordBreakProperty};

// State values for the word break state machine. These are
// an implementation detail of UAX#29, not documented in the spec.
state_enum! {
    StartOfText, Any, CR, ALetter, Numeric, HLetter, Katakana,
    ExtendNumLet, WSegSpace, AHLetterMid, Newline, RIOdd, NumericMid, HLetterDQ,
}

impl State {
    /// Returns true if this state represents a deferred break, meaning that whether we break or not depends on the next character.
    pub const fn is_deferred(self) -> bool {
        match self {
            State::AHLetterMid | State::NumericMid | State::HLetterDQ => true,
            State::StartOfText
            | State::Any
            | State::CR
            | State::ALetter
            | State::Numeric
            | State::HLetter
            | State::Katakana
            | State::ExtendNumLet
            | State::WSegSpace
            | State::RIOdd
            | State::Newline => false,
        }
    }
}

/// A transition in the word break state machine, which consists of a new state and an action to take.
///
/// Note: State needs to be ignored if Action is Transparent.
#[derive(Clone, Copy)]
pub(crate) struct Transition(pub(crate) State, pub(crate) Action);

/// A row is a mapping from property values to transitions.
type Row = [Transition; WordBreakProperty::NUM_VARIANTS];

/// The all-important transition table, which defines the state machine.
pub(crate) const TABLE: [Row; State::NUM_VARIANTS] = [
    start_of_text_transitions(),
    any_transitions(),
    cr_transitions(),
    aletter_transitions(),
    numeric_transitions(),
    hletter_transitions(),
    katakana_transitions(),
    extendnumlet_transitions(),
    wsegspace_transitions(),
    ahletter_mid_transitions(),
    newline_transitions(),
    ri_odd_transitions(),
    numeric_mid_transitions(),
    hletter_dq_transitions(),
];

const fn default_all_break() -> Row {
    let mut row = [brk(State::Any); WordBreakProperty::NUM_VARIANTS];

    // Default transitions to specific states for certain common properties.
    row[WordBreakProperty::ALetter as usize] = brk(State::ALetter);
    row[WordBreakProperty::Numeric as usize] = brk(State::Numeric);
    row[WordBreakProperty::CR as usize] = brk(State::CR);
    row[WordBreakProperty::HebrewLetter as usize] = brk(State::HLetter);
    row[WordBreakProperty::WSegSpace as usize] = brk(State::WSegSpace);
    row[WordBreakProperty::Katakana as usize] = brk(State::Katakana);
    row[WordBreakProperty::ExtendNumLet as usize] = brk(State::ExtendNumLet);

    // WB4: Format and Extend characters don't affect word boundaries, so we treat them as transparent.
    row[WordBreakProperty::Format as usize] = transparent();
    row[WordBreakProperty::Extend as usize] = transparent();
    row[WordBreakProperty::ZWJ as usize] = transparent();
    row[WordBreakProperty::LF as usize] = brk(State::Newline);
    row[WordBreakProperty::Newline as usize] = brk(State::Newline);

    // WB15 & 16: Regional Indicators are handled specially
    row[WordBreakProperty::RegionalIndicator as usize] = brk(State::RIOdd);

    row
}

const fn default_all_deferred() -> Row {
    let mut row = [deferred(State::Any); WordBreakProperty::NUM_VARIANTS];
    row[WordBreakProperty::Format as usize] = transparent();
    row[WordBreakProperty::Extend as usize] = transparent();
    row[WordBreakProperty::ZWJ as usize] = transparent();
    row
}

/// Override Extend/Format back to Break — used by states exempt from WB4
/// (StartOfText, CR, Newline)
const fn without_wb4(row: &mut Row) {
    row[WordBreakProperty::Extend as usize] = brk(State::Any);
    row[WordBreakProperty::Format as usize] = brk(State::Any);
    row[WordBreakProperty::ZWJ as usize] = brk(State::Any);
}

// State::StartOfText
const fn start_of_text_transitions() -> Row {
    let mut row = default_all_break();
    without_wb4(&mut row);
    row
}

// State::Any
const fn any_transitions() -> Row {
    default_all_break()
}

// State::CR — WB3: CR × LF
const fn cr_transitions() -> Row {
    let mut row = default_all_break();
    row[WordBreakProperty::LF as usize] = nb(State::Newline); // WB3
    without_wb4(&mut row);
    row
}

// State::ALetter
const fn aletter_transitions() -> Row {
    let mut row = default_all_break();

    // WB5: AHLetter × AHLetter
    row[WordBreakProperty::ALetter as usize] = nb(State::ALetter);
    row[WordBreakProperty::HebrewLetter as usize] = nb(State::HLetter);

    // WB9: ALetter × Numeric
    row[WordBreakProperty::Numeric as usize] = nb(State::Numeric);

    // WB6: AHLetter	×	(MidLetter | MidNumLetQ) AHLetter
    row[WordBreakProperty::MidLetter as usize] = nb(State::AHLetterMid);
    row[WordBreakProperty::MidNumLet as usize] = nb(State::AHLetterMid);
    row[WordBreakProperty::SingleQuote as usize] = nb(State::AHLetterMid);

    // WB13a: (AHLetter | Numeric | Katakana | ExtendNumLet) × ExtendNumLet
    row[WordBreakProperty::ExtendNumLet as usize] = nb(State::ExtendNumLet);

    row
}

// State::Numeric
const fn numeric_transitions() -> Row {
    let mut row = default_all_break();

    // WB8: Numeric × Numeric
    row[WordBreakProperty::Numeric as usize] = nb(State::Numeric);

    // WB10: Numeric × AHLetter
    row[WordBreakProperty::HebrewLetter as usize] = nb(State::HLetter);
    row[WordBreakProperty::ALetter as usize] = nb(State::ALetter);

    // WB12: Numeric × (MidNum | MidNumLetQ) Numeric
    row[WordBreakProperty::MidNum as usize] = nb(State::NumericMid);
    row[WordBreakProperty::MidNumLet as usize] = nb(State::NumericMid);
    row[WordBreakProperty::SingleQuote as usize] = nb(State::NumericMid);

    // WB13a
    row[WordBreakProperty::ExtendNumLet as usize] = nb(State::ExtendNumLet);

    row
}

// State::HLetter (Hebrew_Letter)
const fn hletter_transitions() -> Row {
    let mut row = default_all_break();

    // WB5: AHLetter × AHLetter
    row[WordBreakProperty::HebrewLetter as usize] = nb(State::HLetter);
    row[WordBreakProperty::ALetter as usize] = nb(State::ALetter);

    // WB9: Numeric × AHLetter
    row[WordBreakProperty::Numeric as usize] = nb(State::Numeric);

    // WB6: AHLetter	×	(MidLetter | MidNumLetQ) AHLetter
    row[WordBreakProperty::MidLetter as usize] = nb(State::AHLetterMid);
    row[WordBreakProperty::MidNumLet as usize] = nb(State::AHLetterMid);
    row[WordBreakProperty::SingleQuote as usize] = nb(State::AHLetterMid);

    // WB7a: Hebrew_Letter × Single_Quote
    row[WordBreakProperty::SingleQuote as usize] = nb(State::Any);

    // WB7b: Hebrew_Letter × Double_Quote Hebrew_Letter
    row[WordBreakProperty::DoubleQuote as usize] = nb(State::HLetterDQ);

    // WB13a
    row[WordBreakProperty::ExtendNumLet as usize] = nb(State::ExtendNumLet);

    row
}

// State::Katakana — WB13: Katakana × Katakana
const fn katakana_transitions() -> Row {
    let mut row = default_all_break();

    // WB13: Katakana × Katakana
    row[WordBreakProperty::Katakana as usize] = nb(State::Katakana);

    // WB13a
    row[WordBreakProperty::ExtendNumLet as usize] = nb(State::ExtendNumLet);

    row
}

// State::ExtendNumLet — WB13a + WB13b
// ExtendNumLet is the glue: it connects to letters, numbers, katakana, and itself.
const fn extendnumlet_transitions() -> Row {
    let mut row = default_all_break();

    // WB13b: ExtendNumLet × (AHLetter | Numeric | Katakana)
    row[WordBreakProperty::ALetter as usize] = nb(State::ALetter);
    row[WordBreakProperty::HebrewLetter as usize] = nb(State::HLetter);
    row[WordBreakProperty::Numeric as usize] = nb(State::Numeric);
    row[WordBreakProperty::Katakana as usize] = nb(State::Katakana);

    // WB13a: ExtendNumLet × ExtendNumLet
    row[WordBreakProperty::ExtendNumLet as usize] = nb(State::ExtendNumLet);

    row
}

// State::WSegSpace
const fn wsegspace_transitions() -> Row {
    let mut row = default_all_break();

    // WB3d: WSegSpace × WSegSpace
    // Keep horizontal whitespace together.
    row[WordBreakProperty::WSegSpace as usize] = nb(State::WSegSpace);

    // WB3d is ordered before WB4 in the spec, so transparency should NOT
    // apply: WSegSpace + Extend + WSegSpace must break. Override the default
    // transparent entries to consume the char but leave WSegSpace state.
    row[WordBreakProperty::Extend as usize] = nb(State::Any);
    row[WordBreakProperty::Format as usize] = nb(State::Any);
    row[WordBreakProperty::ZWJ as usize] = nb(State::Any);

    row
}

// Helper state for handling WB6/WB7
// AHLetter	×	(MidLetter | MidNumLetQ) AHLetter
// AHLetter (MidLetter | MidNumLetQ)	×	AHLetter
const fn ahletter_mid_transitions() -> Row {
    let mut row = default_all_deferred();

    // By default, all transitions from this state are deferred break, e.g.
    // we'll break before the apostrophe in "can't", unless we see a letter after it, in which case we won't break.
    row[WordBreakProperty::ALetter as usize] = nb(State::ALetter);
    row[WordBreakProperty::HebrewLetter as usize] = nb(State::HLetter);

    row
}

// State::Newline
const fn newline_transitions() -> Row {
    let mut row = default_all_break();
    without_wb4(&mut row);
    row
}

// Helper state for handling WB11/WB12
// Numeric × (MidNum | MidNumLetQ) Numeric
// Numeric (MidNum | MidNumLetQ) × Numeric
const fn numeric_mid_transitions() -> Row {
    let mut row = default_all_deferred();
    row[WordBreakProperty::Numeric as usize] = nb(State::Numeric);
    row
}

// Helper state for handling WB7b/WB7c
// Hebrew_Letter × Double_Quote Hebrew_Letter
// Hebrew_Letter Double_Quote × Hebrew_Letter
const fn hletter_dq_transitions() -> Row {
    let mut row = default_all_deferred();
    row[WordBreakProperty::HebrewLetter as usize] = nb(State::HLetter);
    row
}

// State::RIOdd: we track the number of preceding Regional Indicators to implement WB{15,16}
// Do not break within emoji flag sequences. That is, do not break between regional indicator (RI)
// symbols if there is an odd number of RI characters before the break point.
// WB15	sot (RI RI)* RI	×	RI
// WB16	[^RI] (RI RI)* RI	×	RI
const fn ri_odd_transitions() -> Row {
    let mut row = default_all_break();
    row[WordBreakProperty::RegionalIndicator as usize] = nb(State::Any);
    row
}

const fn nb(s: State) -> Transition {
    Transition(s, Action::NoBreak)
}

const fn brk(s: State) -> Transition {
    Transition(s, Action::Break)
}

const fn deferred(s: State) -> Transition {
    Transition(s, Action::DeferredBreak)
}

const fn transparent() -> Transition {
    Transition(State::Any, Action::Transparent)
}
