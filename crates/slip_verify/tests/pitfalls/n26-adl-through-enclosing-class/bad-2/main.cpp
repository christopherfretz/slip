#include "dep.h"

namespace acme {
struct Mine {};
void advance(legacy::list<Mine>::iterator &) {}
}

void caller() {
  legacy::list<acme::Mine>::iterator it;
  advance(it);
}
