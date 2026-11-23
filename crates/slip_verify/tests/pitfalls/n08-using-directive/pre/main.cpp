#include "dep.h"

namespace legacy {
void make(int) {}
}

using namespace legacy;

void user() {
  Dep d;
  make(1);
}
