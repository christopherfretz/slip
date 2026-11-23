#include "dep.h"

namespace acme {
void make(int) {}
}

using namespace legacy;

void user() {
  Dep d;
  make(1);
}
