// Functions copied unchanged from https://github.com/google/gson at commit e67037d17f94415179066c8301fee4a610d9515e.
// Apache-2.0. Copyright (C) 2008-2010 Google Inc.
// The license text is in ../NOTICE. Names a function uses that the project defines elsewhere are left undefined. The methods are unchanged; they are indented into a class so the file parses.

package corpus;

final class Gson {
// From google/gson gson/src/main/java/com/google/gson/internal/LinkedTreeMap.java:530 at e67037d17f94.
// @complexity time=O(n) space=O(1) conf=heuristic
    public Node<K, V> first() {
      Node<K, V> node = this;
      Node<K, V> child = node.left;
      while (child != null) {
        node = child;
        child = node.left;
      }
      return node;
    }

// From google/gson gson/src/main/java/com/google/gson/stream/JsonReader.java:1267 at e67037d17f94.
// @complexity time=O(n) space=O(1) conf=verified
    private String validateString(String value) throws IOException {
      if (strictness != Strictness.STRICT) {
        return value;
      }
      for (int i = 0; i < value.length(); i++) {
        char c = value.charAt(i);
        if (Character.isSurrogate(c)) {
          if (Character.isHighSurrogate(c)
              && i + 1 < value.length()
              && Character.isLowSurrogate(value.charAt(++i))) {
            continue;
          }
          throw syntaxError("Unpaired surrogate characters are not allowed in strict mode");
        }
      }
      return value;
    }

// From google/gson gson/src/main/java/com/google/gson/stream/JsonReader.java:1785 at e67037d17f94.
// @complexity time=O(n) space=O(n) conf=heuristic
    private String getPath(boolean usePreviousPath) {
      StringBuilder result = new StringBuilder().append('$');
      for (int i = 0; i < stackSize; i++) {
        int scope = stack[i];
        switch (scope) {
          case JsonScope.EMPTY_ARRAY:
          case JsonScope.NONEMPTY_ARRAY:
            int pathIndex = pathIndices[i];
            // If index is last path element it points to next array element; have to decrement
            if (usePreviousPath && pathIndex > 0 && i == stackSize - 1) {
              pathIndex--;
            }
            result.append('[').append(pathIndex).append(']');
            break;
          case JsonScope.EMPTY_OBJECT:
          case JsonScope.DANGLING_NAME:
          case JsonScope.NONEMPTY_OBJECT:
            result.append('.');
            if (pathNames[i] != null) {
              result.append(pathNames[i]);
            }
            break;
          case JsonScope.NONEMPTY_DOCUMENT:
          case JsonScope.EMPTY_DOCUMENT:
          case JsonScope.CLOSED:
            break;
          default:
            throw new AssertionError("Unknown scope value: " + scope);
        }
      }
      return result.toString();
    }

// From google/gson gson/src/main/java/com/google/gson/internal/GsonTypes.java:472 at e67037d17f94.
// @complexity time=O(n) space=O(1) conf=verified
    private static int indexOf(Object[] array, Object toFind) {
      for (int i = 0, length = array.length; i < length; i++) {
        if (toFind.equals(array[i])) {
          return i;
        }
      }
      throw new NoSuchElementException();
    }

// From google/gson gson/src/main/java/com/google/gson/stream/JsonWriter.java:743 at e67037d17f94.
// @complexity time=O(n) space=O(1) conf=verified
    private void string(String value) throws IOException {
      String[] replacements = htmlSafe ? HTML_SAFE_REPLACEMENT_CHARS : REPLACEMENT_CHARS;
      out.write('\"');
      int last = 0;
      int length = value.length();
      for (int i = 0; i < length; i++) {
        char c = value.charAt(i);
        String replacement;
        if (c < 128) {
          replacement = replacements[c];
          if (replacement == null) {
            continue;
          }
        } else if (c == '\u2028') {
          replacement = "\\u2028";
        } else if (c == '\u2029') {
          replacement = "\\u2029";
        } else {
          continue;
        }
        if (last < i) {
          out.write(value, last, i - last);
        }
        out.write(replacement);
        last = i + 1;
      }
      if (last < length) {
        out.write(value, last, length - last);
      }
      out.write('\"');
    }
}
