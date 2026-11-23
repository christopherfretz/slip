#include "dep.h"

namespace acme {
struct Mine {};
} namespace legacy {
void prepare(acme::Mine &) {}
}

void caller() {
  legacy::store<acme::Mine> s;
  acme::Mine m;
  s.put(m);
}
