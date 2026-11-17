export class Feeder {
}
export class DogFeeder extends Feeder {
  feed() { return "kibble"; }
}
export class CatFeeder extends Feeder {
  feed() { return "fish"; }
}
export class QuietFeeder extends Feeder {
  feed() {
    return super.feed();
  }
}
export function firstWay(rows) {
  const total = rows.length;
  const doubled = total * 2;
  return doubled + 1;
}
export function secondWay(rows) {
  const total = rows.length;
  const doubled = total * 2;
  return doubled + 1;
}
export function chained(rows) {
  return Promise.resolve(rows.length).then((count) => {
    return Promise.resolve(count).then((inner) => inner + 1);
  });
}
class Level1 extends Feeder {
  feed() { return "1"; }
}
class Level2 extends Level1 {
}
class Level3 extends Level2 {
}
export class Level4 extends Level3 {
}
export function halved(total) {
  // TODO: work out what happens to the remainder
  return 7 / 2 + total;
}
export function ignored(rows) {
  const _ = countRows(rows);
  return rows.length;
}
function countRows(rows) {
  return rows.length;
}
