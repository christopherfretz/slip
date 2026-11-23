#include "dep.h"

namespace app {
struct Mine {};
void ready(Mine &) {}
}

void caller() {
  lib::store<app::Mine> s;
  app::Mine m;
  s.put(m);
}
