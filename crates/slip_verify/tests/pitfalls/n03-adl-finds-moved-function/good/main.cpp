#include "dep.h"

namespace acme {
void use(legacy::Dep) {}
}

void caller() {
  legacy::Dep d;
  acme::use(d);
}
