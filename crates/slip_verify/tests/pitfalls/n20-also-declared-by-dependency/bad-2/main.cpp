#include "dep.h"

namespace acme {
struct Mine {};

int counter = 1;
thread_local int depth = 0;
}

int user() {
  acme::Mine m;
  return acme::counter + acme::depth;
}
