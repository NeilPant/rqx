package rqx;

/**
 * @author someone
 * Created on 2019-04-02
 */
public class Precision {
    public String widen(Object input) {
        String raw = (String) input;
        return raw.trim();
    }

    // imprecise-type: two casts in a row are one decision, so this reports once.
    public Integer forced(String input) {
        return (Integer) (Object) input;
    }

    // user name
    public String userName = "ada";

    public Object loose() {
        return new Object();
    }
}
