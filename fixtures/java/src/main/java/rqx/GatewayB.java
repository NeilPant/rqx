package rqx;

import com.stripe.Charge;

public class GatewayB {
    public int collect(int amount) {
        return Charge.of(amount);
    }
}
