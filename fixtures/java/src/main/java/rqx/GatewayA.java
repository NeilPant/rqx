package rqx;

import com.stripe.Charge;

public class GatewayA {
    public int collect(int amount) {
        return Charge.of(amount);
    }
}
