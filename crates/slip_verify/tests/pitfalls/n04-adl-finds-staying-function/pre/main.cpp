#include "dep.h"

namespace legacy {
struct Mine {};
}

void caller() {
  legacy::Mine m;
  touch(m);
}
