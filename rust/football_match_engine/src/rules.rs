#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchRules {
    pub rules_version: String,
    pub regulation_duration_seconds: i32,
    pub half_duration_seconds: i32,
    pub allow_extra_time: bool,
    pub extra_time_duration_seconds: i32,
    pub allow_penalty_shootout: bool,
    pub substitution_limit: i32,
    pub substitution_windows: i32,
    pub card_rules_version: String,
    pub regulation_added_time_cap_seconds: i32,
    pub extra_time_added_time_cap_seconds: i32,
    pub aggregate_home_goal_difference_before_match: i32,
    pub away_goals_rule: bool,
    pub home_away_goals_before_match: i32,
}
impl MatchRules {
    pub fn standard()->Self {Self {rules_version:"football-standard-v1".into(),regulation_duration_seconds:5400,half_duration_seconds:2700,
        allow_extra_time:false,extra_time_duration_seconds:0,allow_penalty_shootout:false,substitution_limit:5,substitution_windows:3,
        card_rules_version:"cards-standard-v1".into(),regulation_added_time_cap_seconds:360,extra_time_added_time_cap_seconds:180,
        aggregate_home_goal_difference_before_match:0,away_goals_rule:false,home_away_goals_before_match:0}}
    pub fn knockout()->Self {let mut r=Self::standard();r.rules_version="football-knockout-v1".into();r.allow_extra_time=true;r.extra_time_duration_seconds=1800;r.allow_penalty_shootout=true;r}
    pub fn two_leg_knockout(aggregate:i32,away_goals_rule:bool,home_away_goals:i32)->Self {
        let mut r=Self::knockout();r.rules_version="football-two-leg-knockout-v1".into();r.aggregate_home_goal_difference_before_match=aggregate;
        r.away_goals_rule=away_goals_rule;r.home_away_goals_before_match=home_away_goals.max(0);r
    }
    fn knockout_tie_is_level(&self,home_goals:i32,away_goals:i32)->bool {
        let difference=self.aggregate_home_goal_difference_before_match + home_goals-away_goals;
        if difference!=0 {return false}
        if !self.away_goals_rule {return true}
        self.home_away_goals_before_match==away_goals
    }
    pub fn requires_extra_time(&self,h:i32,a:i32)->bool {self.allow_extra_time&&self.knockout_tie_is_level(h,a)}
    pub fn requires_shootout(&self,h:i32,a:i32)->bool {self.allow_penalty_shootout&&self.knockout_tie_is_level(h,a)}
    pub fn fingerprint(&self)->u64 {
        use crate::fingerprints::*;
        let mut h=start();
        h=mix_string(h,Some(&self.rules_version)); h=mix_i32(h,self.regulation_duration_seconds);
        h=mix_i32(h,self.half_duration_seconds);h=mix_bool(h,self.allow_extra_time);h=mix_i32(h,self.extra_time_duration_seconds);
        h=mix_bool(h,self.allow_penalty_shootout);h=mix_i32(h,self.substitution_limit);h=mix_i32(h,self.substitution_windows);
        h=mix_string(h,Some(&self.card_rules_version));h=mix_i32(h,self.regulation_added_time_cap_seconds);
        h=mix_i32(h,self.extra_time_added_time_cap_seconds);h=mix_i32(h,self.aggregate_home_goal_difference_before_match);
        h=mix_bool(h,self.away_goals_rule);h=mix_i32(h,self.home_away_goals_before_match);h
    }
}
