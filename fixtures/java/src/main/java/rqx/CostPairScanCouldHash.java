package rqx;

import java.util.ArrayList;
import java.util.List;

public class CostPairScanCouldHash {
    public static class Order {
        public int id;
        public int userId;
    }

    public static class User {
        public int id;
        public String name;
    }

    public List<String> ownersOf(List<Order> orders, List<User> users) {
        List<String> names = new ArrayList<>();
        for (Order order : orders) {
            for (User user : users) {
                if (order.userId == user.id) {
                    names.add(user.name);
                }
            }
        }
        return names;
    }

    public int sameOrder(List<Order> orders) {
        int pairs = 0;
        for (Order left : orders) {
            for (Order right : orders) {
                if (left.id == right.userId) {
                    pairs += 1;
                }
            }
        }
        return pairs;
    }
}
