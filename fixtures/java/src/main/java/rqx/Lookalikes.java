package rqx;

// Code that resembles a finding and is not one. Every rule should stay silent here.
public class Lookalikes {
    // output-parameter: `factor` in `counted` is a local that shares its name with
    // this parameter, not this parameter.
    public int scaled(int factor) {
        return factor * 2;
    }

    public int counted(String[] items) {
        int factor = items.length;
        factor += 1;
        return factor;
    }

    // mixed-language-file: an error message that names an update is not SQL.
    public String updateFailure(String id) {
        return "Failed to update user " + id;
    }
}
