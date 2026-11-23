#include "dep.h"

namespace acme {
struct Mine {};

void f() {}
}

void user() {
  acme::Mine m;
  acme::f();
}
