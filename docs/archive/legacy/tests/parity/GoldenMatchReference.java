package pl.mkrstudio.truefootball3.simulation.v3.core;
/** Original Java V3, never a Rust result. 20 action-minute 90-minute clock.
 * Golden output intended for full-Rust port's future end-to-end acceptance.
 */
public final class GoldenMatchReference {
    private GoldenMatchReference(){}
    public static void main(String[] args) {
        long[] seeds={123456789L,3168002L,20260822L};
        for(long seed:seeds){
            MatchInput input=CoreRulesSelfTest.input(seed,MatchRules.standard(),55,55,55,55,55,55,55,74,74).withSimulatedActionMinutes(20);
            long start=System.nanoTime();
            MatchResult result=new UnifiedMatchEngine().simulate(input);
            System.out.println(seed+","+result.getHomeGoals()+","+result.getAwayGoals()+","+result.getFinalGameSecond()+","+result.getEventCount()+","+result.getEventTimelineFingerprint()+","+result.getFingerprint()+","+result.getEvents().size()+","+(System.nanoTime()-start)/1_000_000L);
        }
    }
}
