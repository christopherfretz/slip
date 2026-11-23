#include "dep.h"

namespace acme {
struct Mine : legacy::util::Tag { int v[2]; };

int *begin(Mine &m) { return m.v; }
int *end(Mine &m) { return m.v + 2; }
}

int sum(acme::Mine &m) {
  int n = 0;
  for (int x : m) n += x;
  return n;
}
