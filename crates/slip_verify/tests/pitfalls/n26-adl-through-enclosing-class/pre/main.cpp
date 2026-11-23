#include "dep.h"

namespace legacy {
struct Mine {};
void advance(legacy::list<Mine>::iterator &) {}
}

void caller() {
  legacy::list<legacy::Mine>::iterator it;
  advance(it);
}
