package rqx;

public class Config {
    public String endpoint(String region, boolean secure, int retries) {
        if (region.length() > 0) {
            if (secure) {
                if (retries > 0) {
                    return "https://api.example.com/v2/orders";
                }
            }
        }
        return "";
    }

    public int score(int a, int b, int c, int d) {
        return (a + b) * (c - d) + Math.max(a, b) + Math.min(c, d) - (a * d);
    }
}
