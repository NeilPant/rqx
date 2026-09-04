package rqx;

class Flow {
    boolean restated(boolean ready) {
        if (ready) {
            return true;
        } else {
            return false;
        }
    }

    int compared(boolean ready, int count) {
        if (ready == true) {
            return 1;
        }
        if (count == count) {
            return 2;
        }
        return 0;
    }

    int sameEitherWay(int count) {
        if (count > 10) {
            return 99;
        } else {
            return 99;
        }
    }

    void decidedNothing(int count) {
        if (count > 10) {
        }
    }
}
