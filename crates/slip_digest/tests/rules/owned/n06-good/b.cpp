#include "dep.h"

namespace acme {
struct Mine {};
void prepare(Mine &) {}
}

void caller() {
  legacy::store<acme::Mine> s;
  acme::Mine m;
  s.put(m);
}
