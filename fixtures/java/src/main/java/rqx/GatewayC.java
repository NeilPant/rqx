package rqx;

import com.stripe.Charge;

public class GatewayC {
    public int collect(int amount) {
        return Charge.of(amount);
    }
}
