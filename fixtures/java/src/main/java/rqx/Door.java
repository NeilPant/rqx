package rqx;

public class Door {
    private String state = "closed";

    public boolean open() {
        if (this.state == "closed") {
            this.state = "open";
            return true;
        } else if (this.state == "open") {
            return false;
        } else if (this.state == "locked") {
            return false;
        }
        return false;
    }

    public boolean lock(int key) {
        if (this.state == "closed") {
            this.state = "locked";
            return key > 0;
        } else if (this.state == "open") {
            return false;
        }
        return false;
    }
}
