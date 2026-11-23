#include "dep.h"

namespace legacy {
struct Mine {
  friend void poke(Mine) {}
};
}

void user() {
  legacy::Mine m;
  poke(m);
}
