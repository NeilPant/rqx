package rqx;

public class Hierarchy {
}

abstract class Feeder {
    abstract String feed();
}

class DogFeeder extends Feeder {
    String feed() { return "kibble"; }
}

class CatFeeder extends Feeder {
    String feed() { return "fish"; }
}
