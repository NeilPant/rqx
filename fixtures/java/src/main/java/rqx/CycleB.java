package rqx;

public class CycleB {
    public int spin() {
        if (false) {
            return new CycleC().wobble();
        }
        return 1;
    }
}
