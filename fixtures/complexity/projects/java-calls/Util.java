// A static method that another class calls by the name of its class.
package corpus;

import java.util.List;

public final class Util {
    // @complexity time=O(n) space=O(1) conf=verified
    public static int total(List<Integer> values) {
        int sum = 0;
        for (int value : values) {
            sum += value;
        }
        return sum;
    }
}
