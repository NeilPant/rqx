package rqx;

interface Notifier {
    void send(String message);

    void close();
}

class LoggingNotifier implements Notifier {
    private final Notifier inner;

    LoggingNotifier(Notifier inner) {
        this.inner = inner;
    }

    public void send(String message) {
        this.inner.send(message);
    }

    public void close() {
        this.inner.close();
    }

    public int history() {
        return 1;
    }
}

class GuardedNotifier implements Notifier {
    private final Notifier inner;

    GuardedNotifier(Notifier inner) {
        this.inner = inner;
    }

    public void send(String message) {
        this.inner.send(message);
    }

    public void close() {
        this.inner.close();
    }
}

class SmsGateway {
    public int transmit(String text) {
        return text.length();
    }

    public int disconnect() {
        return 0;
    }
}

class SmsNotifier implements Notifier {
    private final SmsGateway gateway;

    SmsNotifier(SmsGateway gateway) {
        this.gateway = gateway;
    }

    public void send(String message) {
        this.gateway.transmit(message);
    }

    public void close() {
        this.gateway.disconnect();
    }
}

class RetryingGateway {
    private final SmsGateway gateway;

    RetryingGateway(SmsGateway gateway) {
        this.gateway = gateway;
    }

    public int transmit(String text) {
        return this.gateway.transmit(text);
    }

    public int disconnect() {
        return this.gateway.disconnect();
    }
}
