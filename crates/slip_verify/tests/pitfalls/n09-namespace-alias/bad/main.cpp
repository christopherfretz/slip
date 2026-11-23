#include "dep.h"

namespace acme {
struct Mine {};
}

namespace L = acme;


void user() {
  L::Dep d;
  L::Mine m;
}
