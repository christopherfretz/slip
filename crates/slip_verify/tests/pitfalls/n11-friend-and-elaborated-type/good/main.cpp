#include "dep.h"

namespace acme {
struct Mine {
  friend void poke(Mine) {}
};
}

void user() {
  acme::Mine m;
  poke(m);
}
