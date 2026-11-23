#include "dep.h"

namespace acme {
struct Mine {};
} namespace legacy {
int counter = 1;
thread_local int depth = 0;
}

int user() {
  acme::Mine m;
  return legacy::counter + legacy::depth;
}
