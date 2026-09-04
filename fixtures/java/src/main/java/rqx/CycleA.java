package rqx;

public class CycleA {
    public int orbit() {
        return new CycleB().spin();
    }
}
