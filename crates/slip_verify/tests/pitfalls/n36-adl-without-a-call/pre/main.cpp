#include "dep.h"

namespace legacy {
struct Mine : legacy::util::Tag { int v[2]; };

int *begin(Mine &m) { return m.v; }
int *end(Mine &m) { return m.v + 2; }
}

int sum(legacy::Mine &m) {
  int n = 0;
  for (int x : m) n += x;
  return n;
}
