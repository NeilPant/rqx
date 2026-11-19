package rqx;

import java.util.HashSet;
import java.util.Set;
import java.util.function.IntConsumer;
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

    // unused-parameter: a varargs parameter is named by its declarator, not by its
    // type.
    public int visits(String... pages) {
        return seen.size() + pages.length;
    }

    // unused-parameter: a protected method is a hook a subclass may replace, and its
    // parameter list is the hook's.
    protected int capacity(int requested) {
        return seen.size();
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

    // unused-parameter: an anonymous class's method answers for the interface it
    // implements, which owns its parameter list.
    public IntConsumer quiet() {
        return new IntConsumer() {
            public void accept(int value) {
                System.out.println("quiet");
            }
        };
    }
}
