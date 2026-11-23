#include "dep.h"

namespace app {
struct Mine {};
void prepare(Mine &) {}
}

void caller() {
  lib::store<app::Mine> s;
  app::Mine m;
  s.put(m);
}
