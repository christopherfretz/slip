#include "dep.h"

namespace acme {
struct Mine {};
}

namespace L = legacy;
namespace A = acme;

void user() {
  L::Dep d;
  A::Mine m;
}
