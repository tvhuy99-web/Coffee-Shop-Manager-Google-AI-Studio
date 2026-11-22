package pl.mkrstudio.truefootball3.simulation.v3.core;
public final class ContextualFixturePrinter {
  public static void main(String[] a) {
    PlayerAttributesSnapshot p = new PlayerAttributesSnapshot(
      21,77,91,78,84,86,88,76,93,90,71,87,82,89,83,85,73,91,64,55);
    System.out.println("attrsFingerprint,"+p.fingerprint());
    System.out.println("pass,"+Double.toHexString(ContextualAttributeModel.passExecutionAdjustment(p,
      new ContextualAttributeModel.PassContext(44,53,24.5,15,76,37,48))));
    System.out.println("cross,"+Double.toHexString(ContextualAttributeModel.crossExecutionAdjustment(p,
      new ContextualAttributeModel.CrossContext(42,33.5,75))));
    System.out.println("shot,"+Double.toHexString(ContextualAttributeModel.shotExecutionAdjustment(p,
      new ContextualAttributeModel.ShotContext(62,84,65,3,false,false))));
    System.out.println("header,"+Double.toHexString(ContextualAttributeModel.shotExecutionAdjustment(p,
      new ContextualAttributeModel.ShotContext(52,93,72,2,true,false))));
    System.out.println("penalty,"+Double.toHexString(ContextualAttributeModel.shotExecutionAdjustment(p,
      new ContextualAttributeModel.ShotContext(4,88,100,0,false,true))));
    System.out.println("groundAttacker,"+Double.toHexString(ContextualAttributeModel.groundDuelAttacker(p,
      new ContextualAttributeModel.GroundDuelContext(true,54,63,65,79))));
    System.out.println("groundDefender,"+Double.toHexString(ContextualAttributeModel.groundDuelDefender(p,
      new ContextualAttributeModel.GroundDuelContext(true,54,63,65,79))));
    System.out.println("pressure,"+Double.toHexString(ContextualAttributeModel.pressureAttributeAdjustment(p,
      new ContextualAttributeModel.PressureContext(8.4,15.7,82,65,55))));
  }
}
