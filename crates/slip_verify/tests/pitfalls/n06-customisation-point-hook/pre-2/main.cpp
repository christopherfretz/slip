#include "dep.h"

namespace legacy {
struct Mine {};
void prepare(Mine &) {}
}

void caller() {
  legacy::store<legacy::Mine> s;
  legacy::Mine m;
  s.put(m);
}
