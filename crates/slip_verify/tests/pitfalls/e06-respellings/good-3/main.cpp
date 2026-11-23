namespace lib {
struct Item {
  int v;
  Item *next;
};
}
namespace L = lib;
using lib::Item;
typedef lib::Item ItemT;
typedef const lib::Item ItemC;

int peek(lib::Item &) { return 1; }
int peek(const lib::Item &) { return 2; }

int take(lib::Item *p) {
  lib::Item copy = *p;
  return peek(copy);
}
