package rqx;

// Code that resembles a finding and is not one. Every rule should stay silent here.
public class Lookalikes {
    // magic-number: each 30 below is the value of a declaration, which names it.
    public int timeoutBudget() {
        final int connectTimeoutSeconds = 30;
        final int readTimeoutSeconds = 30;
        final int idleTimeoutSeconds = 30;
        return connectTimeoutSeconds + readTimeoutSeconds + idleTimeoutSeconds;
    }

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

    // no-base-case: `total(items)` calls the overload that takes a starting value.
    public int total(String[] items) {
        return total(items, 0);
    }

    public int total(String[] items, int start) {
        return start + items.length;
    }

    // distant-declaration: the table is read while it is declared, and used on the next line.
    public int spelled(String word) {
        final java.util.List<String> codeWords = java.util.List.of(
            "alfa",
            "bravo",
            "charlie",
            "delta",
            "echo",
            "foxtrot",
            "golf",
            "hotel",
            "india",
            "juliett",
            "kilo",
            "lima",
            "mike",
            "november",
            "oscar",
            "papa",
            "quebec",
            "romeo",
            "sierra",
            "tango",
            "uniform",
            "victor",
            "whiskey",
            "xray",
            "yankee",
            "zulu"
        );
        return codeWords.indexOf(word);
    }

    // never-reassigned: a loop variable or a catch parameter already marked final.
    public int longest(String[] items) {
        int longest = 0;
        for (final String item : items) {
            longest = Math.max(longest, item.length());
        }
        try {
            return Math.addExact(longest, 1);
        } catch (final ArithmeticException overflow) {
            return overflow.getMessage().length();
        }
    }

    // mixed-language-file: an error message that names an update is not SQL.
    public String updateFailure(String id) {
        return "Failed to update user " + id;
    }

    // empty-catch: a catch that says why it does nothing has handled the error.
    public int parsedOrZero(String text) {
        try {
            return Integer.decode(text);
        } catch (NumberFormatException malformed) {
            // Malformed input counts as zero, which every caller expects.
        }
        return 0;
    }

    // unused-parameter: an annotated method is called by whatever reads the
    // annotation, with the parameters that reader passes.
    @Deprecated
    public String label(String locale) {
        return "lookalike";
    }
}
