package rqx;

public class CostDeepRecursion {
    public int sumTo(int n) {
        if (n <= 0) {
            return 0;
        }
        return n + sumTo(n - 1);
    }

    public int bits(int n) {
        if (n <= 1) {
            return 0;
        }
        return 1 + bits(n >> 1);
    }
}
