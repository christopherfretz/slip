#include "dep.h"

namespace acme {
struct Derived : legacy::Base {};
}

int caller() {
  acme::Derived d;
  return legacy::visit(d);
}
