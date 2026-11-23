#include "dep.h"

namespace acme {
struct Mine {};
}

void caller() {
  acme::Mine m;
  legacy::touch(m);
}
