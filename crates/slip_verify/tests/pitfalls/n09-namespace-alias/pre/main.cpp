#include "dep.h"

namespace legacy {
struct Mine {};
}

namespace L = legacy;


void user() {
  L::Dep d;
  L::Mine m;
}
