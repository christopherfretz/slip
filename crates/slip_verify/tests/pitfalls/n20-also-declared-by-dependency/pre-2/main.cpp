#include "dep.h"

namespace legacy {
struct Mine {};

int counter = 1;
thread_local int depth = 0;
}

int user() {
  legacy::Mine m;
  return legacy::counter + legacy::depth;
}
