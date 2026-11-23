#include "dep.h"
void touch(...) {}
namespace acme {
struct Mine {};
}

void caller() {
  acme::Mine m;
  touch(m);
}
