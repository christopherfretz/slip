#include "dep.h"

namespace acme {
struct Mine {};
} namespace legacy {
void f() {}
}

void user() {
  acme::Mine m;
  legacy::f();
}
