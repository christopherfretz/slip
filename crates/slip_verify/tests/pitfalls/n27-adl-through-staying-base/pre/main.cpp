#include "dep.h"

namespace legacy {
struct Derived : legacy::Base {};
}

int caller() {
  legacy::Derived d;
  return visit(d);
}
