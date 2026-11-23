#include "dep.h"

namespace legacy {
struct Mine {
  friend int poke(Mine);
};

int poke(Mine) { return 1; }
}

int user() {
  legacy::Mine m;
  return poke(m);
}
