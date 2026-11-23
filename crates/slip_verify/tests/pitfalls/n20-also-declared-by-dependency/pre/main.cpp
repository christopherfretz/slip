#include "dep.h"

namespace legacy {
struct Mine {};

void f() {}
}

void user() {
  legacy::Mine m;
  legacy::f();
}
