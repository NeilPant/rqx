package rqx;

public class Router {
    private boolean isHealthCheck(String path) {
        return path.equals("/health");
    }

    private boolean looksLikeUpload(String body) {
        return body.length() > 1024;
    }

    private boolean hasSession(String session) {
        return !session.isEmpty();
    }

    public String route(String path, String body, String session) {
        if (this.isHealthCheck(path)) {
            return "ok";
        } else if (this.looksLikeUpload(body)) {
            return "stored";
        } else if (this.hasSession(session)) {
            return "page";
        }
        return "denied";
    }
}
