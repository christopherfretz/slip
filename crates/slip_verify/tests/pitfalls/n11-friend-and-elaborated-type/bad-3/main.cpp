#include "dep.h"

namespace acme {
struct Mine {
  friend int poke(Mine);
};
} namespace legacy {
int poke(acme::Mine) { return 1; }
}

int user() {
  acme::Mine m;
  return poke(m);
}
