#include "dep.h"
void use(...) {}
namespace acme {
void use(legacy::Dep) {}
}

void caller() {
  legacy::Dep d;
  acme::use(d);
}
