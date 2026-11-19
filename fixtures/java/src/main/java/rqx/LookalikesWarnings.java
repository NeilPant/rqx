package rqx;

import java.util.HashSet;
import java.util.Set;
import java.util.function.Predicate;

// Code that resembles a warning and is not one. Every rule should stay silent here.
public class LookalikesWarnings {
    // self-built-collaborators, working-constructor: a set is where the object keeps
    // its data, not a collaborator a test would replace, and making an empty one is
    // not work.
    private final Set<String> seen;

    public LookalikesWarnings() {
        this.seen = new HashSet<>();
    }

    public boolean isFirstVisit(String page) {
        final boolean first = !seen.contains(page);
        seen.add(page);
        return first;
    }
}

// assertionless-test: `test` is the method a Predicate answers with, not a test case;
// no annotation marks it as one.
class LongEnough implements Predicate<String> {
    @Override
    public boolean test(String value) {
        return value.length() > 3;
    }
}
