package rqx;

public class HierarchyFood {
}

abstract class Food {
    abstract String label();
}

class DogFood extends Food {
    String label() { return "dog"; }
}

class CatFood extends Food {
    String label() { return "cat"; }
}
