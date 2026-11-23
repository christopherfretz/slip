#include "dep.h"

namespace acme {
class Mine {
  int s = 1;
  friend struct Dep;
};
}

int user() { return legacy::Dep::get(acme::Mine()); }
