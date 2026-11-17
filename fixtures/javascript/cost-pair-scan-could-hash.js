function ownersOf(orders, users) {
  const names = [];
  for (const order of orders) {
    for (const user of users) {
      if (order.userId === user.id) {
        names.push(user.name);
      }
    }
  }
  return names;
}

function sameOrder(orders) {
  let pairs = 0;
  for (const left of orders) {
    for (const right of orders) {
      if (left.id === right.userId) {
        pairs += 1;
      }
    }
  }
  return pairs;
}
