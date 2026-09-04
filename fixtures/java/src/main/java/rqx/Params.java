package rqx;

public class Params {
    public int withDeadTail(int used, String dead) {
        return used;
    }

    public int withDeadMiddle(String dead, int used) {
        return used;
    }

    public int respectsUnderscore(int used, String _deliberate) {
        return used;
    }

    public int twoDeadOnOneLine(int used, String deadA, String deadB) {
        return used;
    }
}
