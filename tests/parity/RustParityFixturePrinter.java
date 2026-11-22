package pl.mkrstudio.truefootball3.simulation.v3.core;

/** Reference fixture from UNMODIFIED Java engine primitive implementations. */
public final class RustParityFixturePrinter {
    public static void main(String[] args) {
        for (long seed : new long[]{1L,42L,123456789L,-27L}) {
            DeterministicRng r = new DeterministicRng(seed);
            System.out.println("rng," + seed + "," + r.nextLong()+","+r.nextInt(1000)+","+r.nextBoolean()+","+Double.toHexString(r.nextDouble()));
            System.out.println("fork," + seed + "," + DeterministicRng.deriveSeed(seed,"BÌNH LUẬN 🎙️"));
        }
        long h=Fingerprints.start();h=Fingerprints.mix(h,"Xin chào 🌍");
        h=Fingerprints.mix(h,123);h=Fingerprints.mix(h,-18L);
        System.out.println("fingerprint,"+h);
        System.out.println("rules_standard,"+MatchRules.standard().fingerprint());
        System.out.println("rules_knockout,"+MatchRules.knockout().fingerprint());
        System.out.println("width,"+SpatialMath.scaleWidth(31,33));
        System.out.println("distance,"+SpatialMath.distance(20,10,43,67));
    }
}
