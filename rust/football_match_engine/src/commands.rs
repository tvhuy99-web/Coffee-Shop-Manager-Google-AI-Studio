use crate::{
    fingerprints as fp,
    geometry::TeamSide,
    input::{SetPiecePlanSnapshot, TacticalSlotSnapshot},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum CommandType {
    Substitute = 0,
    ReassignSlot = 1,
    AssignGoalkeeper = 2,
    ChangeShape = 3,
    ChangePressing = 4,
    ChangeTempo = 5,
    ChangeWidth = 6,
    ChangeLineHeight = 7,
    ChangeTeamInstruction = 8,
    ChangeSlotInstruction = 9,
    ChangeSetPiecePlan = 10,
    ChangePlayerMorale = 11,
}

impl CommandType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Substitute => "SUBSTITUTE",
            Self::ReassignSlot => "REASSIGN_SLOT",
            Self::AssignGoalkeeper => "ASSIGN_GOALKEEPER",
            Self::ChangeShape => "CHANGE_SHAPE",
            Self::ChangePressing => "CHANGE_PRESSING",
            Self::ChangeTempo => "CHANGE_TEMPO",
            Self::ChangeWidth => "CHANGE_WIDTH",
            Self::ChangeLineHeight => "CHANGE_LINE_HEIGHT",
            Self::ChangeTeamInstruction => "CHANGE_TEAM_INSTRUCTION",
            Self::ChangeSlotInstruction => "CHANGE_SLOT_INSTRUCTION",
            Self::ChangeSetPiecePlan => "CHANGE_SET_PIECE_PLAN",
            Self::ChangePlayerMorale => "CHANGE_PLAYER_MORALE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchCommand {
    pub game_second: i32,
    pub side: TeamSide,
    pub kind: CommandType,
    pub player_id: Option<String>,
    pub secondary_player_id: Option<String>,
    pub slot_id: Option<String>,
    pub int_value: i32,
    pub string_value: Option<String>,
    pub tactical_slots: Vec<TacticalSlotSnapshot>,
    pub set_piece_plan: Option<SetPiecePlanSnapshot>,
}

impl MatchCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        game_second: i32,
        side: TeamSide,
        kind: CommandType,
        player_id: Option<String>,
        secondary_player_id: Option<String>,
        slot_id: Option<String>,
        int_value: i32,
        string_value: Option<String>,
        tactical_slots: Vec<TacticalSlotSnapshot>,
        set_piece_plan: Option<SetPiecePlanSnapshot>,
    ) -> Result<Self, &'static str> {
        if game_second < 0 {
            return Err("gameSecond cannot be negative");
        }
        Ok(Self {
            game_second,
            side,
            kind,
            player_id,
            secondary_player_id,
            slot_id,
            int_value,
            string_value,
            tactical_slots,
            set_piece_plan,
        })
    }

    pub fn change_tempo(second: i32, side: TeamSide, tempo: i32) -> Result<Self, &'static str> {
        Self::new(second, side, CommandType::ChangeTempo, None, None, None, tempo, None, Vec::new(), None)
    }

    pub fn substitute(second: i32, side: TeamSide, out_id: impl Into<String>, in_id: impl Into<String>) -> Result<Self, &'static str> {
        Self::new(second, side, CommandType::Substitute, Some(out_id.into()), Some(in_id.into()), None, 0, None, Vec::new(), None)
    }

    pub fn reassign_slot(second: i32, side: TeamSide, player_id: impl Into<String>, slot_id: impl Into<String>) -> Result<Self, &'static str> {
        Self::new(second, side, CommandType::ReassignSlot, Some(player_id.into()), None, Some(slot_id.into()), 0, None, Vec::new(), None)
    }

    pub fn assign_goalkeeper(second: i32, side: TeamSide, player_id: impl Into<String>) -> Result<Self, &'static str> {
        Self::new(second, side, CommandType::AssignGoalkeeper, Some(player_id.into()), None, None, 0, None, Vec::new(), None)
    }

    pub fn change_shape(second: i32, side: TeamSide, shape_id: impl Into<String>, slots: Vec<TacticalSlotSnapshot>) -> Result<Self, &'static str> {
        Self::new(second, side, CommandType::ChangeShape, None, None, None, 0, Some(shape_id.into()), slots, None)
    }

    pub fn change_set_piece_plan(second: i32, side: TeamSide, plan: SetPiecePlanSnapshot) -> Result<Self, &'static str> {
        Self::new(second, side, CommandType::ChangeSetPiecePlan, None, None, None, 0, None, Vec::new(), Some(plan))
    }

    pub fn change_player_morale(second: i32, side: TeamSide, player_id: impl Into<String>, morale: i32) -> Result<Self, &'static str> {
        Self::new(second, side, CommandType::ChangePlayerMorale, Some(player_id.into()), None, None, morale, None, Vec::new(), None)
    }

    pub fn fingerprint(&self) -> u64 {
        let mut h = fp::start();
        h = fp::mix_i32(h, self.game_second);
        h = fp::mix_i32(h, self.side as i32);
        h = fp::mix_i32(h, self.kind as i32);
        h = fp::mix_string(h, self.player_id.as_deref());
        h = fp::mix_string(h, self.secondary_player_id.as_deref());
        h = fp::mix_string(h, self.slot_id.as_deref());
        h = fp::mix_i32(h, self.int_value);
        h = fp::mix_string(h, self.string_value.as_deref());
        for s in &self.tactical_slots {
            h = fp::mix_i64(h, s.fingerprint() as i64);
        }
        let sp = self.set_piece_plan.as_ref().map(|p| p.fingerprint() as i64).unwrap_or(0);
        fp::mix_i64(h, sp)
    }
}
