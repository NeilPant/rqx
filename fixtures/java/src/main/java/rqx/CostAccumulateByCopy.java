package rqx;

import java.util.List;

public class CostAccumulateByCopy {
    public String joinWithPlus(List<String> parts) {
        String text = "";
        for (String part : parts) {
            text += part;
        }
        return text;
    }

    public String joinWithBuilder(List<String> parts) {
        StringBuilder text = new StringBuilder();
        for (String part : parts) {
            text.append(part);
        }
        return text.toString();
    }

    public int lengthOf(List<String> parts) {
        int length = 0;
        for (String part : parts) {
            length += part.length();
        }
        return length;
    }
}
