package rqx;

import java.util.HashSet;
import java.util.Set;
import java.util.function.IntPredicate;

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

// unused-private-member: `codeOf` is called through another instance of the class,
// and `serialVersionUID` is read by serialization, not by the source.
class Codes implements java.io.Serializable {
    private static final long serialVersionUID = 1L;

    private int codeOf(char letter) {
        return letter - 'a';
    }

    int distance(Codes other, char from, char to) {
        return other.codeOf(to) - other.codeOf(from);
    }
}

// assertionless-test: `test` is the method an IntPredicate answers with, not a test case;
// no annotation marks it as one.
class LongEnough implements IntPredicate {
    @Override
    public boolean test(int value) {
        return value > 3;
    }
}
