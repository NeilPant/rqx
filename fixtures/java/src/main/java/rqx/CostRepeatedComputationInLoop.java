package rqx;

import java.util.ArrayList;
import java.util.List;

public class CostRepeatedComputationInLoop {
    private int totalLength(List<String> parts) {
        int total = 0;
        for (String part : parts) {
            total += part.length();
        }
        return total;
    }

    public List<Double> shares(List<String> parts, List<Double> weights) {
        List<Double> out = new ArrayList<>();
        for (Double weight : weights) {
            int whole = totalLength(parts);
            out.add(weight / whole);
        }
        return out;
    }

    public List<Double> sharesOfEach(List<Double> weights) {
        List<Double> out = new ArrayList<>();
        for (Double weight : weights) {
            List<String> own = List.of(String.valueOf(weight));
            out.add(weight / totalLength(own));
        }
        return out;
    }
}
