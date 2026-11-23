#include "dep.h"

namespace legacy {
void use(legacy::Dep) {}
}

void caller() {
  legacy::Dep d;
  use(d);
}
