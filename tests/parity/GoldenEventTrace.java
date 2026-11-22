package pl.mkrstudio.truefootball3.simulation.v3.core;
/** Deterministic, event-by-event original-Java fixture. Event fingerprints
 * include actor/target, positions and insertion-ordered metadata.
 */
public final class GoldenEventTrace {
    private GoldenEventTrace(){}
    public static void main(String[] args) {
        long[] seeds={123456789L,3168002L,20260822L};
        System.out.println("seed,sequence,game_second,event_type,team,event_fingerprint");
        for(long seed:seeds) {
            MatchInput in=CoreRulesSelfTest.input(seed,MatchRules.standard(),55,55,55,55,55,55,55,74,74).withSimulatedActionMinutes(20);
            MatchResult r=new UnifiedMatchEngine().simulate(in);
            for(MatchEvent ev:r.getEvents()){
                System.out.println(seed+","+ev.getSequence()+","+ev.getGameSecond()+","+ev.getType().name()+","+
                    (ev.getSide()==null?"NONE":ev.getSide().name())+","+ev.fingerprint());
            }
            if(r.getEventCount()!=r.getEvents().size())throw new AssertionError("trace truncated");
        }
    }
}
